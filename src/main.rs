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

// 'error_chain!' can recurse deeply
#![recursion_limit = "1024"]

mod errors
{
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! { }
}

mod cohorthist;
mod commitdb;
mod common;
mod gitcommitreader;
mod plotter;
mod projectmeta;
mod statuslogger;

use std::path::PathBuf;
use std::process::Command;
use structopt::StructOpt;
use errors::*;
use crate::commitdb::CommitDb;
use crate::common::{ CohortType, IntervalType, UnitType };
use crate::gitcommitreader::GitCommitReader;
use crate::plotter::Plotter;
use crate::projectmeta::ProjectMeta;
use crate::statuslogger::StatusLogger;

#[macro_use]
extern crate error_chain;

/* ---------------------- *
 * Command-line arguments *
 * ---------------------- */

#[derive(StructOpt, Debug)]
struct Args
{
    /// Path to project metadata JSON file
    #[structopt(short, long, parse(from_os_str))]
    meta: Option<PathBuf>,

    #[structopt(subcommand)]
    cmd: MainCommand
}

#[derive(StructOpt, Debug)]
enum MainCommand
{
    Ingest
    {
        /// Path to SQLite database (will be created if nonexistent)
        #[structopt(parse(from_os_str))]
        db_path: PathBuf,

        /// Paths to Git repositories to ingest
        #[structopt(parse(from_os_str))]
        repo_tree_paths: Vec<PathBuf>
    },
    Plot
    {
        /// Path to SQLite database previously created by ingestion
        #[structopt(parse(from_os_str))]
        db_path: PathBuf,

        /// Output path for PNG image
        #[structopt(parse(from_os_str))]
        out_path: PathBuf,

        /// Cohorts to use (firstyear, domain, repo, prefix or suffix)
        #[structopt(short, long, default_value = "firstyear")]
        cohort: CohortType,

        /// Y axis data type (authors, commits, or changes)
        #[structopt(short, long, default_value = "authors")]
        unit: UnitType,

        /// X axis granularity (month or year)
        #[structopt(short, long, default_value = "year")]
        interval: IntervalType,

        /// First year to show
        #[structopt(short, long)]
        from: Option<i32>,

        /// Last year to show
        #[structopt(short, long)]
        to: Option<i32>
    }
}

/* ---- *
 * Main *
 * ---- */

fn main()
{
    if let Err(ref e) = run()
    {
        eprintln!("error: {}", e);

        for e in e.iter().skip(1)
        {
            eprintln!("caused by: {}", e);
        }

        // Run with `RUST_BACKTRACE=1` to get a backtrace.

        if let Some(backtrace) = e.backtrace()
        {
            eprintln!("backtrace: {:?}", backtrace);
        }

        ::std::process::exit(1);
    }
}

fn run() -> Result<()>
{
    let args = Args::from_args();
    let meta = 
        match args.meta
        {
            Some(m) => { ProjectMeta::from_file(&m)? },
            None => { ProjectMeta::new() }
        };

    match args.cmd
    {
        MainCommand::Ingest { db_path, repo_tree_paths } =>
        {
            run_ingest(db_path, repo_tree_paths, &meta)
        },
        MainCommand::Plot { db_path, out_path, cohort, unit, interval, from, to } =>
        {
            run_plot(db_path, out_path, &meta, cohort, unit, interval, from, to)
        }
    }
}

fn run_ingest(db_path: PathBuf, repo_tree_paths: Vec<PathBuf>, _meta: &ProjectMeta) -> Result<()>
{
    let mut cdb = CommitDb::open(db_path).unwrap();
    let mut sl = StatusLogger::new();

    for path in repo_tree_paths.iter()
    {
        let repo_name =
            path.canonicalize().unwrap()
            .file_name().unwrap()
            .to_string_lossy()
            .into_owned();

        sl.begin_repo(&repo_name);

        // Check for promisor for origin remote; we interpret its presence
        // as a preference for remote storage. If found, we turn off --stat
        // collection, since that would cause git to fetch all the remote
        // blobs (slowly).
        //
        // This will break change counts. Author and commit counts will still
        // work.

        let mut cmd;
        cmd = Command::new("git");
        cmd.arg("-C").arg(&path).arg("config").arg("remote.origin.promisor");
        let output = cmd.output().unwrap();
        let has_promisor = std::str::from_utf8(&output.stdout).unwrap().trim() == "true";

        if has_promisor
        {
            sl.log_warning("origin has a promisor; change details omitted.");
        }

        let gcr = GitCommitReader::new(path.clone(),
                                       &repo_name,
                                       cdb.get_last_author_time(&repo_name),
                                       !has_promisor)?;

        for commit in gcr
        {
            cdb.insert_raw_commit(&commit)?;
            sl.log_commit(&commit);
        }

        sl.end_repo();
    }

    Ok(())
}

fn run_plot(db_path: PathBuf, out_path: PathBuf, meta: &ProjectMeta,
            cohort: CohortType, unit: UnitType, interval: IntervalType,
            from: Option<i32>, to: Option<i32>) -> Result<()>
{
    let mut cdb = CommitDb::open(db_path)?;
    cdb.postprocess(&meta.domains)?; // FIXME: Skip if metadata is unchanged
    let hist = cdb.get_hist(cohort, unit, interval).chain_err(|| "")?;
    let plotter = Plotter { };

    match interval
    {
        IntervalType::Month =>
        {
            plotter.plot_monthly_cohorts(&meta, &unit.to_string(), &hist, &out_path, from, to)
        },
        IntervalType::Year =>
        {
            plotter.plot_yearly_cohorts(&meta, &unit.to_string(), &hist, &out_path, from, to)
        }
    }
}
