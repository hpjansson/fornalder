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

/* ------------ *
 * StatusLogger *
 * ------------ */

use chrono::Datelike;
use chrono::prelude::Utc;
use std::io;
use std::io::Write;
use crate::gitcommitreader::RawCommit;

pub struct StatusLogger
{
    repo_name: String,
    n_commits: u32,
    last_timestamp: i64,
    last_year: i32,
    last_month: i32
}

impl StatusLogger
{
    pub fn new() -> StatusLogger
    {
        StatusLogger
        {
            repo_name: "".to_string(),
            n_commits: 0,
            last_timestamp: 0,
            last_year: 0,
            last_month: 0,
        }
    }

    pub fn begin_repo(&mut self, repo_name: &String)
    {
        self.repo_name = repo_name.clone();
        self.n_commits = 0;
        self.last_timestamp = 0;
        self.last_year = 0;
        self.last_month = 0;

        eprint!("{}: \x1b[K", self.repo_name);
        io::stdout().flush().unwrap();
    }

    pub fn log_commit(&mut self, commit: &RawCommit)
    {
        self.n_commits += 1;

        if commit.author_time.is_none() { return; }

        let author_year = commit.author_time.unwrap().year();
        let author_month = commit.author_time.unwrap().month0() as i32;
        let timestamp = Utc::now().timestamp_millis();

        if timestamp - self.last_timestamp > 500
            || author_year != self.last_year
            || author_month != self.last_month
        {
            eprint!("\r{}: {}-{:02} ({} commits)\x1b[K",
                   self.repo_name,
                   author_year,
                   author_month + 1,
                   self.n_commits);
            io::stderr().flush().unwrap();

            self.last_timestamp = timestamp;
            self.last_year = author_year;
            self.last_month = author_month;
        }
    }

    pub fn end_repo(&mut self)
    {
        if self.last_year != 0
        {
            eprint!("\r{}: {}-{:02} ({} commits)\x1b[K\n",
                   self.repo_name,
                   self.last_year,
                   self.last_month + 1,
                   self.n_commits);
        }
        else
        {
            eprint!("\r{}: {} commits\x1b[K\n",
                   self.repo_name,
                   self.n_commits);
        }

        io::stderr().flush().unwrap();
    }
}
