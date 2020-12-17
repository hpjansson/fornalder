/* -*- Mode: rust; tab-width: 4; indent-tabs-mode: nil; c-basic-offset: 4 -*- */

/* Copyright (C) 2020 Hans Petter Jansson
 *
 * This file is part of Fornalder, a program that visualizes long-term trends
 * in contributions to version control repositories.
 *
 * Fornalder is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published
 * by the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * Fornalder is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with Fornalder.  If not, see <http://www.gnu.org/licenses/>. */

/* --------------- *
 * GitCommitReader *
 * --------------- */

use chrono::prelude::Utc;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use std::io::{BufRead, BufReader, Split};
use std::iter::Peekable;
use std::process::{Command, Stdio, ChildStdout};
use crate::errors::*;

#[derive(PartialEq, Default, Clone, Debug)]
pub struct RawCommit
{
    pub id: String,
    pub repo_name: String,
    pub author_name: String,
    pub author_email: String,
    pub author_time: Option<DateTime::<FixedOffset>>,
    pub committer_name: String,
    pub committer_email: String,
    pub committer_time: Option<DateTime::<FixedOffset>>,
    pub n_insertions: i32,
    pub n_deletions: i32
}

pub struct GitCommitReader
{
    repo_name: String,
    insertions_re: Regex,
    deletions_re: Regex,
    commit_re: Regex,
    line_splitter: Peekable<Split<BufReader<ChildStdout>>>
}

impl GitCommitReader
{
    pub fn new(repo_path: std::path::PathBuf, repo_name: &str, since: DateTime<Utc>) -> Result<GitCommitReader>
    {
        let repo_path = repo_path.canonicalize().unwrap();
        let stdout = Command::new("git")
            .arg("--git-dir")
            .arg(repo_path.join(".git"))
            .arg("--work-tree")
            .arg(&repo_path)
            .arg("log")
            .arg("--branches")
            .arg("--remotes")
            .arg("--pretty=format:%H__sep__%aD__sep__%an__sep__%ae__sep__%cD__sep__%cn__sep__%ce")
            .arg("--reverse")
            .arg("--since")
            .arg(since.to_rfc2822())
            .arg("--date-order")
            .arg("--shortstat")
            .arg("HEAD")
            .stdout(Stdio::piped())
            .spawn().chain_err(|| "Could not spawn git")?
            .stdout.chain_err(|| "Could not read git output")?;
        let reader = BufReader::new(stdout);

        let gcr: GitCommitReader = GitCommitReader
        {
            repo_name: repo_name.to_string(),
            insertions_re: Regex::new(r"([0-9]+) insertions?").unwrap(),
            deletions_re: Regex::new(r"([0-9]+) deletions?").unwrap(),
            commit_re: Regex::new(r"^[0-9a-f]+__sep__").unwrap(),
            line_splitter: reader.split(b'\n').peekable()
        };

        Ok(gcr)
    }
}

impl Iterator for GitCommitReader
{
    type Item = RawCommit;

    fn next(&mut self) -> Option<Self::Item>
    {
        let mut commit: RawCommit = RawCommit::default();

        // Find the first line of commit entry

        let mut seg = self.line_splitter.next();
        while seg.is_some()
        {
            let line = String::from_utf8_lossy(&seg.unwrap().unwrap()).to_string();

            if self.commit_re.is_match(&line)
            {
                let split = line.split("__sep__").map(|x| x.to_string()).collect::<Vec<String>>();

                commit.id = split[0].clone();
                commit.repo_name = self.repo_name.clone();
                commit.author_time = Some(DateTime::parse_from_rfc2822(&split[1]).unwrap());
                commit.author_name = split[2].clone();
                commit.author_email = split[3].to_lowercase();
                commit.committer_time = Some(DateTime::parse_from_rfc2822(&split[4]).unwrap());
                commit.committer_name = split[5].clone();
                commit.committer_email = split[6].to_lowercase();
                break;
            }

            seg = self.line_splitter.next();
        }

        // Get optional insertions/deletions stats. We need to peek here
        // so as not to throw out the first line of the next commit.

        let mut next_seg = self.line_splitter.peek();
        while next_seg.is_some()
        {
            let line = String::from_utf8_lossy(&next_seg.unwrap().as_ref().unwrap());

            if self.commit_re.is_match(&line) { break; }

            if self.insertions_re.is_match(&line)
            {
                commit.n_insertions += self.insertions_re.captures(&line).unwrap()[1].parse::<i32>().unwrap();
            }
            if self.deletions_re.is_match(&line)
            {
                commit.n_deletions += self.deletions_re.captures(&line).unwrap()[1].parse::<i32>().unwrap();
            }

            self.line_splitter.next();
            next_seg = self.line_splitter.peek();
        }

        if commit.id.is_empty() { return None; }
        Some(commit)
    }
}
