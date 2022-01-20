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

/* -------- *
 * CommitDb *
 * -------- */

use chrono::prelude::Utc;
use chrono::{ Datelike, DateTime, NaiveDateTime };
use rusqlite::{ Connection, NO_PARAMS };
use crate::cohorthist::{ CohortHist, NO_COHORT, YearMonth };
use crate::common::{ CohortType, IntervalType, UnitType };
use crate::errors::*;
use crate::gitcommitreader::RawCommit;
use crate::projectmeta::DomainMeta;

pub struct CommitDb
{
    conn: Connection,
}

impl CommitDb
{
    pub fn open(db_path: std::path::PathBuf) -> Result<CommitDb>
    {
        let conn = Connection::open(db_path).chain_err(|| "Failed to open database")?;

        // Specify a few pragmas to speed SQLite up by a whole lot.
        for (a, b) in
            &[ ("temp_store", "memory"),
               ("cache_size", "16384"),
               ("locking_mode", "exclusive"),
               ("synchronous", "normal"),
               ("journal_mode", "WAL"),
               ("wal_autocheckpoint", "10000"),
               ("journal_size_limit", "10000000") ]
        {
            conn.pragma_update(None, a, &b.to_string()).chain_err(|| "Failed to set pragma")?;
        }

        conn.execute_batch("
            create table if not exists raw_commits (
                id text primary key on conflict replace,
                repo_name text not null,
                author_name text,
                author_email text,
                author_domain text,
                author_time int,
                author_year int,
                author_month int,
                committer_name text,
                committer_email text,
                committer_time int,
                n_insertions int,
                n_deletions int,
                show_domain bool);
            create index if not exists index_repo_name on raw_commits (repo_name);
            create index if not exists index_author_name on raw_commits (author_name);
            create index if not exists index_author_email on raw_commits (author_email);
            create index if not exists index_author_domain on raw_commits (author_domain);
            create index if not exists index_author_time on raw_commits (author_time);
            create index if not exists index_author_year on raw_commits (author_year);
            create index if not exists index_author_month on raw_commits (author_month);
            create index if not exists index_committer_name on raw_commits (committer_name);
            create index if not exists index_committer_email on raw_commits (committer_email);
            create index if not exists index_committer_time on raw_commits (committer_time);

            create table if not exists suffixes (
                commit_oid int,
                suffix text,
                n_changes int);
            create index if not exists index_suffix on suffixes (suffix);
        ").chain_err(|| "Failed to create tables")?;

        Ok(CommitDb { conn })
    }

    pub fn insert_raw_commit(&mut self, commit: &RawCommit) -> Result<()>
    {
        let author_time: i64;
        let author_year: i32;
        let author_month: i32;
        let committer_time: i64;

        if commit.author_time.is_some()
        {
            author_time = commit.author_time.unwrap().timestamp();
            author_year = commit.author_time.unwrap().year();
            author_month = commit.author_time.unwrap().month0() as i32;
        }
        else
        {
            author_time = 0;
            author_year = 1970;
            author_month = 0;
        }

        if commit.committer_time.is_some()
        {
            committer_time = commit.committer_time.unwrap().timestamp();
        }
        else
        {
            committer_time = 0;
        }

        let mut insert_raw_commit_stmt = self.conn.prepare_cached("
            insert into raw_commits (
                id,
                repo_name,
                author_name,
                author_email,
                author_domain,
                author_time,
                author_year,
                author_month,
                committer_name,
                committer_email,
                committer_time,
                n_insertions,
                n_deletions,
                show_domain
             ) values
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, true)
        ").unwrap();
        insert_raw_commit_stmt.execute (
            &[&commit.id,
              &commit.repo_name,
              &commit.author_name,
              &commit.author_email,
              &email_to_domain(&commit.author_email),
              &author_time.to_string(),
              &author_year.to_string(),
              &author_month.to_string(),
              &commit.committer_name,
              &commit.committer_email,
              &committer_time.to_string(),
              &commit.n_insertions.to_string(),
              &commit.n_deletions.to_string()]).chain_err(|| "Failed to insert commit")?;

        let commit_oid: String = self.conn.last_insert_rowid().to_string();

        for (suffix, n_changes) in &commit.n_changes_per_suffix {
            let mut insert_suffix_stats_stmt = self.conn.prepare_cached("
                insert into suffixes (
                    commit_oid,
                    suffix,
                    n_changes
                ) values
                ( ?1, ?2, ?3 )
            ").unwrap();
            insert_suffix_stats_stmt.execute (
                &[&commit_oid, suffix, &n_changes.to_string()]
            ).chain_err(|| "Failed to insert suffix stats")?;
        }

        Ok(())
    }

    pub fn postprocess(&mut self, domains: &Option<Vec<DomainMeta>>) -> Result<()>
    {
        // Delete commits with unlikely timestamps. These are brobably broken
        // and would confuse our range detection.

        self.conn.execute (
            format!("delete from raw_commits
                     where author_year < 1980 or author_year > {}",
                    Utc::now().year()).as_str(),
                    NO_PARAMS)
            .chain_err(|| "Failed to trim wayward commits")?;

        // Show all domains by default.

        self.conn.execute("
            update raw_commits
            set show_domain=true;",
            NO_PARAMS).chain_err(|| "Error initializing domain visibility")?;

        if domains.is_some()
        {
            for domain in domains.as_ref().unwrap()
            {
                if domain.aggregate_emails.is_some()
                {
                    self.conn.execute(&format!("
                        update raw_commits
                        set author_domain='{}'
                        where {}",
                        domain.name,
                        domain.sql_emails_selector()),
                        NO_PARAMS).chain_err(|| "Error mapping e-mail pattern to domains")?;
                }

                if domain.show.is_some()
                {
                    let show_domain = domain.show.unwrap();

                    self.conn.execute(&format!("
                        update raw_commits
                        set show_domain={}
                        where author_domain='{}'",
                        show_domain,
                        domain.name),
                        NO_PARAMS).chain_err(|| "Error applying visibility flag to domains")?;
                }
            }
        }

        // Generate table with per-author stats like time of first and
        // last commit.

        self.conn.execute ("drop table authors;", NO_PARAMS).ok();
        self.conn.execute_batch ("
            create table authors as
                select author_name,
                       first_time,
                       first_year,
                       last_time,
                       last_year,
                       last_time-first_time as active_time,
                       n_commits,
                       n_changes
                from
                (
                    select author_name,
                           min(author_time) as first_time,
                           min(author_year) as first_year,
                           max(author_time) as last_time,
                           max(author_year) as last_year,
                           count(id) as n_commits,
                           sum(n_insertions) + sum(n_deletions) as n_changes
                    from raw_commits
                    group by author_name
                );
            create index if not exists index_author_name on authors (author_name);
            create index if not exists index_first_time on authors (first_time);
            create index if not exists index_active_time on authors (active_time);
        ").chain_err(|| "Could not create author summaries")?;

        Ok(())
    }

    pub fn get_last_author_time(&mut self, repo_name: &str) -> DateTime<Utc>
    {
        let mut stmt = self.conn.prepare("
            select author_time from raw_commits
                where repo_name = ?1
                order by author_time desc
                limit 1;").unwrap();

        let rows = stmt.query(&[repo_name]);
        if let Ok(mut rows) = rows
        {
            let row = rows.next().unwrap_or(None);
            if let Some(r) = row
            {
                return DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(r.get_unwrap::<usize, i64>(0), 0), Utc);
            }
        }

        DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(0, 0), Utc)
    }

    fn get_firstyear_hist(&mut self, interval: IntervalType, count_sel: &str) -> Result<CohortHist>
    {
        let interval_str = match interval
        {
            IntervalType::Month => "author_year, author_month",
            _ => "author_year"
        };
        let mut stmt = self.conn.prepare(&format!("
            select {}, first_year, {}
            from raw_commits, authors
            where raw_commits.author_name=authors.author_name
                and active_time > (60*60*24*90)
            group by {}, first_year
            union select {}, {}, {}
            from raw_commits, authors
            where raw_commits.author_name=authors.author_name
                and active_time <= (60*60*24*90)
            group by {};
        ", interval_str,
           count_sel,
           interval_str,
           interval_str,
           NO_COHORT,
           count_sel,
           interval_str)).unwrap();
 
        let mut rows = stmt.query(NO_PARAMS).chain_err(|| "Could not query database")?;
        let mut hist = CohortHist::new();

        while let Some(r) = rows.next().chain_err(|| "Could not query database")?
        {
            match interval
            {
                IntervalType::Month =>
                {
                    hist.set_value(YearMonth { year:  r.get(0).unwrap(),
                                               month: r.get(1).unwrap() },
                                   r.get(2).unwrap(), r.get(3).unwrap());
                    hist.set_cohort_name(r.get(2).unwrap(), &r.get::<_, i32>(2).unwrap().to_string());
                },
                IntervalType::Year =>
                {
                    hist.set_value(YearMonth { year:  r.get(0).unwrap(),
                                               month: None },
                                   r.get(1).unwrap(), r.get(2).unwrap());
                    hist.set_cohort_name(r.get(1).unwrap(), &r.get::<_, i32>(1).unwrap().to_string());
                }
            }
        }

        hist.set_cohort_name(NO_COHORT, &"Brief".to_string());

        Ok(hist)
    }

    fn get_column_hist(&mut self, column: &str, interval: IntervalType, count_sel: &str) -> Result<CohortHist>
    {
        const N_ITEMS: i32 = 15;
        let interval_str = match interval
        {
            IntervalType::Month => "author_year, author_month",
            _ => "author_year"
        };
        self.conn.execute (&format!("drop table {column}_top;", column = column), NO_PARAMS).ok();
        self.conn.execute (&format!("
            create table {column}_top as
                select raw_commits.{column} as {column},row_number() over(order by {count_selector} desc) as rowid
                from raw_commits, authors
                where raw_commits.author_name = authors.author_name
                    and raw_commits.show_domain = true
                    and active_time > (60*60*24*90)
                group by {column}
                order by {count_selector} desc
                limit {n_items};",
            column = column,
            count_selector = count_sel,
            n_items = N_ITEMS),
            NO_PARAMS).chain_err(|| format!("Could not generate {}_top", column))?;
        let mut stmt = self.conn.prepare(&(format!("
            select {interval}, {last_item}-{column}_top.rowid, {count_selector}, {column}_top.{column}
            from {column}_top, raw_commits, authors
            where raw_commits.{column} = {column}_top.{column}
                and raw_commits.author_name = authors.author_name
                and active_time > (60*60*24*90)
            group by {interval}, {column}_top.rowid",
            column = column,
            interval = interval_str,
            count_selector = count_sel,
            last_item = N_ITEMS + 1)

            + &format!("

            union

            select {interval},{item_num},{count_selector},\"Other\"
            from raw_commits, authors
            where raw_commits.author_name = authors.author_name
                and {column} not in (select {column} from {column}_top)
                and active_time > (60*60*24*90)
            group by {interval}",
            column = column,
            interval = interval_str,
            count_selector = count_sel,
            item_num = N_ITEMS + 1)

            + &format!("

            union

            select {interval},{item_num},{count_selector},\"Brief\"
            from raw_commits, authors
            where raw_commits.author_name = authors.author_name
                and active_time <= (60*60*24*90)
            group by {interval}",
            interval = interval_str,
            count_selector = count_sel,
            item_num = NO_COHORT)

            + ";")).unwrap();

        let mut rows = stmt.query(NO_PARAMS).chain_err(|| "Could not query database")?;
        let mut hist = CohortHist::new();

        while let Some(r) = rows.next().chain_err(|| "Could not query database")?
        {
            match interval
            {
                IntervalType::Month =>
                {
                    hist.set_value(YearMonth { year:  r.get(0).unwrap(),
                                               month: r.get(1).unwrap() },
                                   r.get(2).unwrap(), r.get(3).unwrap());
                    hist.set_cohort_name(r.get(2).unwrap(), &r.get::<_, String>(4).unwrap());
                },
                IntervalType::Year =>
                {
                    hist.set_value(YearMonth { year:  r.get(0).unwrap(),
                                               month: None },
                                   r.get(1).unwrap(), r.get(2).unwrap());
                    hist.set_cohort_name(r.get(1).unwrap(), &r.get::<_, String>(3).unwrap());
                }
            }
        }

        Ok(hist)
    }

    fn format_column_aggregates_from_where(&self, extra_table: Option<&str>) -> String
    {
        if extra_table.is_some() {
            format!("from raw_commits, {table} where show_domain = true
                     and raw_commits.oid = {table}.commit_oid",
                    table=extra_table.unwrap()).to_string()
        } else {
            "from raw_commits where show_domain = true".to_string()
        }
    }

    fn create_subcommit_year_aggregates(&mut self, column: &str, extra_table: &str) -> Result<()>
    {
        self.conn.execute (&format!("drop table {}_year_aggregates;", column), NO_PARAMS).ok();
        self.conn.execute_batch (&format!("
            create table {column}_year_aggregates as
                select b.author_year as year,
                       b.{column} as {column},
                       sum(cast({column}_count as float)/commit_count) as column_sum
                from
                (
                    select commit_oid,
                           count(*) as commit_count
                    from {table}
                    group by commit_oid
                ) as a,
                (
                    select commit_oid,
                           author_year,
                           {column},
                           count(*) as {column}_count
                    from raw_commits, authors, {table}
                    where show_domain = true
                        and raw_commits.oid = {table}.commit_oid
                        and raw_commits.author_name = authors.author_name
                        and authors.active_time > (60*60*24*90)
                    group by commit_oid,
                             {column}
                ) as b
                where a.commit_oid = b.commit_oid
                group by b.author_year,
                         b.{column};

            create index if not exists index_year on {column}_year_aggregates (year);
            create index if not exists index_{column} on {column}_year_aggregates ({column});
        ", column=column, table=extra_table))
        .chain_err(|| format!("Could not create {} per-year aggregates", column))?;

        Ok(())
    }

    fn create_column_year_aggregates(&mut self, column: &str, extra_table: Option<&str>) -> Result<()>
    {
        let from_where = self.format_column_aggregates_from_where (extra_table);

        self.conn.execute (&format!("drop table {}_year_aggregates;", column), NO_PARAMS).ok();
        self.conn.execute_batch (&format!("
            create table {column}_year_aggregates as
                select b.author_year as year,
                       b.{column} as {column},
                       sum(cast(author_{column}_count as float)/author_count) as active_author_sum
                from authors,
                (
                    select author_year,
                           {column},
                           author_name,
                           count(*) as author_count
                    {from_where}
                    group by author_year,
                             author_name
                ) as a,
                (
                    select author_year,
                           {column},
                           author_name,
                           count(*) as author_{column}_count
                    {from_where}
                    group by author_year,
                             author_name,
                             {column}
                ) as b
                where a.author_year = b.author_year
                    and a.author_name = b.author_name
                    and authors.author_name = b.author_name
                    and authors.active_time > (60*60*24*90)
                group by b.author_year,
                         b.{column};

            create index if not exists index_year on {column}_year_aggregates (year);
            create index if not exists index_{column} on {column}_year_aggregates ({column});
        ", column=column, from_where=from_where))
        .chain_err(|| format!("Could not create {} per-year aggregates", column))?;

        Ok(())
    }

    fn create_column_month_aggregates(&mut self, column: &str, extra_table: Option<&str>) -> Result<()>
    {
        let from_where = self.format_column_aggregates_from_where (extra_table);

        self.conn.execute (&format!("drop table {}_month_aggregates;", column), NO_PARAMS).ok();
        self.conn.execute_batch (&format!("
            create table {column}_month_aggregates as
                select b.author_year as year,
                       b.author_month as month,
                       b.{column} as {column},
                       sum(cast(author_{column}_count as float)/author_count) as active_author_sum
                from authors,
                (
                    select author_year,
                           author_month,
                           {column},
                           author_name,
                           count(*) as author_count
                    {from_where}
                    group by author_year,
                             author_month,
                             author_name
                ) as a,
                (
                    select author_year,
                           author_month,
                           {column},
                           author_name,
                           count(*) as author_{column}_count
                    {from_where}
                    group by author_year,
                             author_month,
                             author_name,
                             {column}
                ) as b
                where a.author_year = b.author_year
                    and a.author_month = b.author_month
                    and a.author_name = b.author_name
                    and authors.author_name = b.author_name
                    and authors.active_time > (60*60*24*90)
                group by b.author_year,
                         b.author_month,
                         b.{column};

            create index if not exists index_year on {column}_month_aggregates (year);
            create index if not exists index_month on {column}_month_aggregates (month);
            create index if not exists index_{column} on {column}_month_aggregates ({column});
        ", column=column, from_where=from_where))
        .chain_err(|| format!("Could not create {} per-month aggregates", column))?;

        Ok(())
    }

    fn get_column_authors_hist(&mut self, column: &str, interval: IntervalType) -> Result<CohortHist>
    {
        const N_ITEMS: i32 = 15;
        let interval_str: &str;
        let author_interval_str: &str;
        let aggregate_table;

        match interval
        {
            IntervalType::Year =>
            {
                interval_str = "year";
                author_interval_str = "author_year";
                aggregate_table = format!("{}_year_aggregates", column);
                if column == "suffix" {
                    self.create_column_year_aggregates(column, Some("suffixes"))?;
                } else {
                    self.create_column_year_aggregates(column, None)?;
                }
            },
            IntervalType::Month =>
            {
                interval_str = "year, month";
                author_interval_str = "author_year, author_month";
                aggregate_table = format!("{}_month_aggregates", column);
                if column == "suffix" {
                    self.create_column_month_aggregates(column, Some("suffixes"))?;
                } else {
                    self.create_column_month_aggregates(column, None)?;
                }
            }
        }

        self.conn.execute (&format!("drop table {column}_top;", column = column), NO_PARAMS).ok();
        self.conn.execute (&format!("
            create table {column}_top as
                select {column} as {column},row_number() over(order by sum(active_author_sum) desc) as rowid
                from {aggregate_table}
                group by {column}
                order by sum(active_author_sum) desc
                limit {n_items};",
            column = column, aggregate_table = aggregate_table, n_items = N_ITEMS),
            NO_PARAMS).chain_err(|| "Could not generate top domains")?;
        let mut stmt = self.conn.prepare(&(format!("
            select {interval}, {n_items}-{column}_top.rowid as ab, sum(active_author_sum) as ac, {column}_top.{column} as ad
            from {column}_top, {aggregate_table}

            where {aggregate_table}.{column} = {column}_top.{column}
            group by {interval}, {column}_top.rowid",
            interval = interval_str,
            n_items = N_ITEMS + 1,
            aggregate_table = aggregate_table,
            column = column)

            // TODO: Optionally hide small cohorts
            + &format!("

            union

            select {interval},{n_items},sum(active_author_sum),\"Other\"
            from {aggregate_table}
            where {column} not in (select {column} from {column}_top)
            group by {interval}",
            interval = interval_str,
            n_items = N_ITEMS + 1,
            aggregate_table = aggregate_table,
            column = column)

            // TODO: Optionally hide brief contributors
            + &format!("

            union

            select {interval},{cohort_num},count(distinct raw_commits.author_name),\"Brief\"
            from raw_commits, authors
            where raw_commits.author_name=authors.author_name
                and show_domain = true
                and active_time <= (60*60*24*90)
            group by {interval}",
            interval = author_interval_str,
            cohort_num = NO_COHORT)

            + ";")).unwrap();

        let mut rows = stmt.query(NO_PARAMS).chain_err(|| "Could not query database")?;
        let mut hist = CohortHist::new();

        while let Some(r) = rows.next().chain_err(|| "Could not query database")?
        {
            match interval
            {
                IntervalType::Month =>
                {
                    hist.set_value(YearMonth { year:  r.get(0).unwrap(),
                                               month: r.get(1).unwrap() },
                                   r.get(2).unwrap(), r.get::<_,f64>(3).unwrap() as i32);
                    hist.set_cohort_name(r.get(2).unwrap(), &r.get::<_, String>(4).unwrap());
                },
                IntervalType::Year =>
                {
                    hist.set_value(YearMonth { year:  r.get(0).unwrap(),
                                               month: None },
                                   r.get(1).unwrap(), r.get::<_,f64>(2).unwrap() as i32);
                    hist.set_cohort_name(r.get(1).unwrap(), &r.get::<_, String>(3).unwrap());
                }
            }
        }

        // TODO: Optionally hide brief contributors
        hist.set_cohort_name(NO_COHORT, &"Brief".to_string());

        Ok(hist)
    }

    fn get_subcommit_hist(&mut self, column: &str, interval: IntervalType) -> Result<CohortHist>
    {
        const N_ITEMS: i32 = 15;
        let interval_str: &str;
        let author_interval_str: &str;
        let aggregate_table;

        match interval
        {
            IntervalType::Year =>
            {
                interval_str = "year";
                author_interval_str = "author_year";
                aggregate_table = format!("{}_year_aggregates", column);
                if column == "suffix" {
                    self.create_subcommit_year_aggregates(column, "suffixes")?;
                }
            },
            IntervalType::Month =>
            {
                interval_str = "year, month";
                author_interval_str = "author_year, author_month";
                aggregate_table = format!("{}_month_aggregates", column);
                if column == "suffix" {
                    self.create_column_month_aggregates(column, Some("suffixes"))?;
                } else {
                    self.create_column_month_aggregates(column, None)?;
                }
            }
        }

        self.conn.execute (&format!("drop table {column}_top;", column = column), NO_PARAMS).ok();
        self.conn.execute (&format!("
            create table {column}_top as
                select {column} as {column},row_number() over(order by sum(column_sum) desc) as rowid
                from {aggregate_table}
                group by {column}
                order by sum(column_sum) desc
                limit {n_items};",
            column = column, aggregate_table = aggregate_table, n_items = N_ITEMS),
            NO_PARAMS).chain_err(|| "Could not generate top domains")?;
        let mut stmt = self.conn.prepare(&(format!("
            select {interval}, {n_items}-{column}_top.rowid as ab, sum(column_sum) as ac, {column}_top.{column} as ad
            from {column}_top, {aggregate_table}

            where {aggregate_table}.{column} = {column}_top.{column}
            group by {interval}, {column}_top.rowid",
            interval = interval_str,
            n_items = N_ITEMS + 1,
            aggregate_table = aggregate_table,
            column = column)

            // TODO: Optionally hide small cohorts
            + &format!("

            union

            select {interval},{n_items},sum(column_sum),\"Other\"
            from {aggregate_table}
            where {column} not in (select {column} from {column}_top)
            group by {interval}",
            interval = interval_str,
            n_items = N_ITEMS + 1,
            aggregate_table = aggregate_table,
            column = column)

            // TODO: Optionally hide brief contributors
            + &format!("

            union

            select {interval},{cohort_num},count(distinct raw_commits.author_name),\"Brief\"
            from raw_commits, authors
            where raw_commits.author_name=authors.author_name
                and show_domain = true
                and active_time <= (60*60*24*90)
            group by {interval}",
            interval = author_interval_str,
            cohort_num = NO_COHORT)

            + ";")).unwrap();

        let mut rows = stmt.query(NO_PARAMS).chain_err(|| "Could not query database")?;
        let mut hist = CohortHist::new();

        while let Some(r) = rows.next().chain_err(|| "Could not query database")?
        {
            match interval
            {
                IntervalType::Month =>
                {
                    hist.set_value(YearMonth { year:  r.get(0).unwrap(),
                                               month: r.get(1).unwrap() },
                                   r.get(2).unwrap(), r.get::<_,f64>(3).unwrap() as i32);
                    hist.set_cohort_name(r.get(2).unwrap(), &r.get::<_, String>(4).unwrap());
                },
                IntervalType::Year =>
                {
                    hist.set_value(YearMonth { year:  r.get(0).unwrap(),
                                               month: None },
                                   r.get(1).unwrap(), r.get::<_,f64>(2).unwrap() as i32);
                    hist.set_cohort_name(r.get(1).unwrap(), &r.get::<_, String>(3).unwrap());
                }
            }
        }

        // TODO: Optionally hide brief contributors
        hist.set_cohort_name(NO_COHORT, &"Brief".to_string());

        Ok(hist)
    }

    pub fn get_hist(&mut self, cohort: CohortType, unit: UnitType,
                    interval: IntervalType) -> Result<CohortHist>
    {
        let selector = match unit
        {
            UnitType::Authors => "count(distinct raw_commits.author_name)",
            UnitType::Commits => "count(*)",
            UnitType::Changes => "sum(n_insertions + n_deletions)"
        };

        match cohort
        {
            CohortType::FirstYear =>
            {
                self.get_firstyear_hist(interval, selector)
            },
            CohortType::Domain =>
            {
                match unit
                {
                    UnitType::Authors => { self.get_column_authors_hist("author_domain", interval) },
                    _ => { self.get_column_hist("author_domain", interval, selector) }
                }
            },
            CohortType::Repo =>
            {
                match unit
                {
                    UnitType::Authors => { self.get_column_authors_hist("repo_name", interval) },
                    _ => { self.get_column_hist("repo_name", interval, selector) }
                }
            }
            CohortType::Suffix =>
            {
                match unit
                {
                    UnitType::Authors => { self.get_column_authors_hist("suffix", interval) },
                    // TODO
                    _ => { self.get_subcommit_hist("suffix", interval) }
                }
            }
        }
    }
}

fn email_to_domain(email: &str) -> String
{
    let mut email: String = email.to_lowercase();

    // Strip local part.

    if let Some(p) = email.rfind('@') {
        email.replace_range(0..=p, "");
    }

    // Trim the domain as much as possible. If the last element looks
    // like a country code and the next-to-last one is 2-3 letters, it's
    // likely of the form 'domain.ac.uk' or 'domain.com.au'. We keep
    // three elements in those cases. Otherwise we keep two as in
    // 'domain.org'.
    //
    // If we wanted to get fancy we could've used this list:
    //
    // https://publicsuffix.org/list/public_suffix_list.dat
    //
    // ...but the relative gain is likely not worth it.

    let split: Vec<&str> = email.split('.').collect();
    let n = split.len();

    if n > 2
    {
        if split[n - 1].len() < 3
        {
            if split[n - 2].len() < 4
            {
                // domain.com.au
                split[n - 3..n].join(".")
            }
            else
            {
                // domain.au
                split[n - 2..n].join(".")
            }
        }
        else
        {
            // domain.org
            split[n - 2..n].join(".")
        }
    }
    else
    {
        // Already optimal, or malformed
        email
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_email_username() {
        assert_eq!(email_to_domain("dude@lebowski.com"), "lebowski.com");
    }
}
