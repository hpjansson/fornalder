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

        conn.execute("
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
            create index index_repo_name on raw_commits (repo_name);
            create index index_author_name on raw_commits (author_name);
            create index index_author_email on raw_commits (author_email);
            create index index_author_domain on raw_commits (author_domain);
            create index index_author_time on raw_commits (author_time);
            create index index_author_year on raw_commits (author_year);
            create index index_author_month on raw_commits (author_month);
            create index index_committer_name on raw_commits (committer_name);
            create index index_committer_email on raw_commits (committer_email);
            create index index_committer_time on raw_commits (committer_time);
        ", NO_PARAMS).chain_err(|| "Failed to create tables")?;

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
        self.conn.execute ("
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
            create index index_author_name on authors (author_name);
            create index index_first_time on authors (first_time);
            create index index_active_time on authors (active_time);
        ", NO_PARAMS).chain_err(|| "Could not create author summaries")?;

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

    fn get_domain_hist(&mut self, interval: IntervalType, count_sel: &str) -> Result<CohortHist>
    {
        const N_DOMAINS: i32 = 15;
        let interval_str = match interval
        {
            IntervalType::Month => "author_year, author_month",
            _ => "author_year"
        };
        self.conn.execute ("drop table top_domains;", NO_PARAMS).ok();
        self.conn.execute (&format!("
            create table top_domains as
                select author_domain as domain,row_number() over(order by {} desc) as rowid
                from raw_commits, authors
                where raw_commits.author_name = authors.author_name
                    and raw_commits.show_domain = true
                    and active_time > (60*60*24*90)
                group by domain
                order by {} desc
                limit {};",
            count_sel, count_sel, N_DOMAINS),
            NO_PARAMS).chain_err(|| "Could not generate top domains")?;
        let mut stmt = self.conn.prepare(&(format!("
            select {}, {}-top_domains.rowid, {}, top_domains.domain
            from top_domains, raw_commits, authors
            where raw_commits.author_domain = top_domains.domain
                and raw_commits.author_name = authors.author_name
                and active_time > (60*60*24*90)
            group by {}, top_domains.rowid",
            interval_str,
            N_DOMAINS + 1,
            count_sel,
            interval_str)

            + &format!("

            union

            select {},{},{},\"Other\"
            from raw_commits, authors
            where raw_commits.author_name = authors.author_name
                and author_domain not in (select domain from top_domains)
                and active_time > (60*60*24*90)
            group by {}",

            interval_str,
            N_DOMAINS + 1,
            count_sel,
            interval_str)

            + &format!("

            union

            select {},{},{},\"Brief\"
            from raw_commits, authors
            where raw_commits.author_name = authors.author_name
                and active_time <= (60*60*24*90)
            group by {}",

            interval_str,
            NO_COHORT,
            count_sel,
            interval_str)

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

    fn create_domain_year_aggregates(&mut self) -> Result<()>
    {
        self.conn.execute ("drop table domain_year_aggregates;", NO_PARAMS).ok();
        self.conn.execute ("
            create table domain_year_aggregates as
                select b.author_year as year,
                       b.author_domain as domain,
                       sum(cast(author_domain_count as float)/author_count) as active_author_sum
                from authors,
                (
                    select author_year,
                           author_domain,
                           author_name,
                           count(*) as author_count
                    from raw_commits
                    where show_domain = true
                    group by author_year,
                             author_name
                ) as a,
                (
                    select author_year,
                           author_domain,
                           author_name,
                           count(*) as author_domain_count
                    from raw_commits
                    where show_domain = true
                    group by author_year,
                             author_name,
                             author_domain
                ) as b
                where a.author_year = b.author_year
                    and a.author_name = b.author_name
                    and authors.author_name = b.author_name
                    and authors.active_time > (60*60*24*90)
                group by b.author_year,
                         b.author_domain;

            create index index_year on domain_year_aggregates (year);
            create index index_domain on domain_year_aggregates (domain);
        ", NO_PARAMS).chain_err(|| "Could not create domain-year aggregates")?;

        Ok(())
    }

    fn create_domain_month_aggregates(&mut self) -> Result<()>
    {
        self.conn.execute ("drop table domain_month_aggregates;", NO_PARAMS).ok();
        self.conn.execute ("
            create table domain_month_aggregates as
                select b.author_year as year,
                       b.author_month as month,
                       b.author_domain as domain,
                       sum(cast(author_domain_count as float)/author_count) as active_author_sum
                from authors,
                (
                    select author_year,
                           author_month,
                           author_domain,
                           author_name,
                           count(*) as author_count
                    from raw_commits
                    where show_domain = true
                    group by author_year,
                             author_month,
                             author_name
                ) as a,
                (
                    select author_year,
                           author_month,
                           author_domain,
                           author_name,
                           count(*) as author_domain_count
                    from raw_commits
                    where show_domain = true
                    group by author_year,
                             author_month,
                             author_name,
                             author_domain
                ) as b
                where a.author_year = b.author_year
                    and a.author_month = b.author_month
                    and a.author_name = b.author_name
                    and authors.author_name = b.author_name
                    and authors.active_time > (60*60*24*90)
                group by b.author_year,
                         b.author_month,
                         b.author_domain;

            create index index_year on domain_month_aggregates (year);
            create index index_month on domain_month_aggregates (month);
            create index index_domain on domain_month_aggregates (domain);
        ", NO_PARAMS).chain_err(|| "Could not create domain-month aggregates")?;

        Ok(())
    }

    fn get_domain_authors_hist(&mut self, interval: IntervalType) -> Result<CohortHist>
    {
        const N_DOMAINS: i32 = 15;
        let interval_str: &str;
        let author_interval_str: &str;
        let domain_aggregate_table: &str;

        match interval
        {
            IntervalType::Year =>
            {
                interval_str = "year";
                author_interval_str = "author_year";
                domain_aggregate_table = "domain_year_aggregates";
                self.create_domain_year_aggregates()?;
            },
            IntervalType::Month =>
            {
                interval_str = "year, month";
                author_interval_str = "author_year, author_month";
                domain_aggregate_table = "domain_month_aggregates";
                self.create_domain_month_aggregates()?;
            }
        }

        self.conn.execute ("drop table top_domains;", NO_PARAMS).ok();
        self.conn.execute (&format!("
            create table top_domains as
                select domain,row_number() over(order by sum(active_author_sum) desc) as rowid
                from {}
                group by domain
                order by sum(active_author_sum) desc
                limit {};",
            domain_aggregate_table, N_DOMAINS),
            NO_PARAMS).chain_err(|| "Could not generate top domains")?;
        let mut stmt = self.conn.prepare(&(format!("
            select {}, {}-top_domains.rowid as ab, sum(active_author_sum) as ac, top_domains.domain as ad
            from top_domains, {}

            where {}.domain = top_domains.domain
            group by {}, top_domains.rowid",
            interval_str,
            N_DOMAINS + 1,
            domain_aggregate_table,
            domain_aggregate_table,
            interval_str)

            // TODO: Optionally hide small cohorts
            + &format!("

            union

            select {},{},sum(active_author_sum),\"Other\"
            from {}
            where domain not in (select domain from top_domains)
            group by {}",
            interval_str,
            N_DOMAINS + 1,
            domain_aggregate_table,
            interval_str)

            // TODO: Optionally hide brief contributors
            + &format!("

            union

            select {},{},count(distinct raw_commits.author_name),\"Brief\"
            from raw_commits, authors
            where raw_commits.author_name=authors.author_name
                and show_domain = true
                and active_time <= (60*60*24*90)
            group by {}",
            author_interval_str,
            NO_COHORT,
            author_interval_str)

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
                    UnitType::Authors => { self.get_domain_authors_hist(interval) },
                    _ => { self.get_domain_hist(interval, selector) }
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
