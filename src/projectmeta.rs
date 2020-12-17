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

/* ----------- *
 * ProjectMeta *
 * ----------- */

use std::fs;
use std::path::*;
use serde::{Deserialize};
use crate::cohorthist::*;
use crate::errors::*;

#[derive(Deserialize, Debug)]
struct Marker
{
    time: YearMonth,
    row: i32,
    text: String
}

#[derive(Deserialize, Debug)]
pub struct AggregatePattern
{
    pattern: String,
    begin: Option<YearMonth>,
    end: Option<YearMonth>
}

impl AggregatePattern
{
    fn sql_selector(&self, string_field: &str, timestamp_field: &str) -> String
    {
        let mut s: String;

        s = format!("({} glob '{}'", string_field, self.pattern);

        if self.begin.is_some()
        {
            s += &format!(" and {} >= {}",
                          timestamp_field,
                          self.begin.unwrap().begin_dt().timestamp());
        }

        if self.end.is_some()
        {
            s += &format!(" and {} < {}",
                          timestamp_field,
                          self.end.unwrap().end_dt().timestamp());
        }

        s + &")".to_string()
    }
}

#[derive(Deserialize, Debug)]
pub struct DomainMeta
{
    pub name: String,
    pub show: Option<bool>,
    pub aggregate_emails: Option<Vec<AggregatePattern>>
}

impl DomainMeta
{
    pub fn sql_emails_selector(&self) -> String
    {
        if self.aggregate_emails.is_none() { return "".to_string(); }

        self.aggregate_emails.as_ref().unwrap().iter()
            .map(|ae| ae.sql_selector("author_email", "author_time")).collect::<Vec<String>>().join(" or ")
    }
}

#[derive(Deserialize, Debug)]
pub struct ProjectMeta
{
    pub name: Option<String>,
    pub first_year: Option<i32>,
    pub last_year: Option<i32>,
    pub domains: Option<Vec<DomainMeta>>,
    markers: Option<Vec<Marker>>
}

impl ProjectMeta
{
    pub fn new() -> ProjectMeta
    {
        ProjectMeta { name: None, first_year: None, last_year: None, markers: None,
                      domains: None }
    }

    pub fn from_file(filename: &PathBuf) -> Result<ProjectMeta>
    {
        let content = fs::read_to_string(filename).chain_err(|| "Could not read meta file")?;
        let pm: ProjectMeta = serde_json::from_str(&content).chain_err(|| "Failed to parse project metadata")?;

        Ok(pm)
    }

    pub fn markers_to_gnuplot(&self) -> (String, i32)
    {
        if self.markers.is_none() || self.markers.as_ref().unwrap().is_empty()
        {
            return ("".to_string(), 0);
        }

        let mut n_markers = 0;

        ("array markers = [ ".to_string()
            + &self.markers.as_ref().unwrap().iter()
                .map(|m| { n_markers += 1;
                           format!("'{}', '{:02}', {}, '{}',",
                                   m.time.year, m.time.month.unwrap_or(-1), m.row, m.text) })
                .collect::<Vec<String>>().join(" ")
            + &" ];".to_string(),
         n_markers)
    }
}
