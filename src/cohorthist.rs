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

/* ---------- *
 * CohortHist *
 * ---------- */

use std::collections::HashMap;
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize};

pub const NO_MONTH: i32 = -1;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Deserialize, Debug)]
pub struct YearMonth
{
    pub year: i32,
    pub month: i32
}

impl YearMonth
{
    pub fn next(&self) -> YearMonth
    {
        let mut ym = YearMonth { year: -1, month: -1 };

        if self.month == NO_MONTH
        {
            ym.year = self.year + 1;
            ym.month = NO_MONTH;
        }
        else if self.month == 11
        {
            ym.year = self.year + 1;
            ym.month = 0;
        }
        else
        {
            ym.year = self.year;
            ym.month = self.month + 1;
        }

        ym
    }

    pub fn begin_dt(&self) -> NaiveDateTime
    {
        let m = if self.month == NO_MONTH { 1 } else { self.month + 1 };
        NaiveDate::from_ymd(self.year, m as u32, 1).and_hms(0, 0, 0)
    }

    pub fn end_dt(&self) -> NaiveDateTime
    {
        if self.month == NO_MONTH
        {
            NaiveDate::from_ymd(self.year + 1, 1, 1).and_hms(0, 0, 0)
        }
        else
        {
            let (y, m) =
                if self.month == 11
                {
                    (self.year + 1, 1)
                }
                else
                {
                    (self.year, self.month + 2)
                };

            NaiveDate::from_ymd(y, m as u32, 1).and_hms(0, 0, 0)
        }
    }
}

pub const NO_COHORT: i32 = -1;

#[derive(Debug)]
pub struct CohortHist
{
    bins: HashMap<YearMonth, HashMap<i32, i32>>,
    first_cohort: i32,
    last_cohort: i32,
    cohort_names: HashMap<i32, String>
}

impl CohortHist
{
    pub fn new() -> CohortHist
    {
        CohortHist
        {
            bins: HashMap::new(),
            first_cohort: i32::MAX,
            last_cohort: i32::MIN,
            cohort_names: HashMap::new()
        }
    }

    pub fn set_value(&mut self, ym: YearMonth, cohort: i32, value: i32)
    {
        // NOTE: This will not work if we're overwriting existing values.

        if cohort != NO_COHORT
        {
            if cohort < self.first_cohort { self.first_cohort = cohort; }
            if cohort > self.last_cohort { self.last_cohort = cohort; }
        }

        self.bins.entry(ym).or_insert_with(HashMap::new).insert(cohort, value);
    }

    pub fn get_value(&self, ym: YearMonth, cohort: i32) -> Option<i32>
    {
        let result = self.bins.get(&ym)?;
        let value = result.get(&cohort);

        match value
        {
            Some(_) => { Some(*value.unwrap()) },
            None => None
        }
    }

    pub fn set_cohort_name(&mut self, cohort: i32, name: &String)
    {
        self.cohort_names.insert(cohort, name.clone());
    }

    pub fn get_cohort_name(&self, cohort: i32) -> String
    {
        let name = self.cohort_names.get(&cohort);
        match name
        {
            Some(_) => { name.unwrap().clone() },
            None => { "".to_string() }
        }
    }

    pub fn get_bounds(&self) -> Option<(YearMonth, YearMonth, i32, i32)>
    {
        let mut first_ym = YearMonth { year: i32::MAX, month: i32::MAX };
        let mut last_ym = YearMonth { year: i32::MIN, month: i32::MIN };

        for ym in self.bins.keys()
        {
            if ym < &first_ym
            {
                first_ym = *ym;
                if last_ym < first_ym { last_ym = first_ym; }
            }

            if ym > &last_ym { last_ym = *ym; }
        }

        match first_ym
        {
            YearMonth { year: i32::MAX, month: i32::MAX } => { None },
            _ => { Some((first_ym, last_ym, self.first_cohort, self.last_cohort)) }
        }
    }

    pub fn get_n_bins(&self) -> i32
    {
        let bounds = self.get_bounds();

        match bounds
        {
            Some(x) => { x.1.year - x.0.year + 1 },
            None => 0
        }
    }

    pub fn to_vecs(&self) -> Vec<(YearMonth, Vec<(i32, i32)>)>
    {
        let mut vecs: Vec<(YearMonth, Vec<(i32, i32)>)> = Vec::new();
        let first_ym: YearMonth;
        let last_ym: YearMonth;
        let first_cohort: i32;
        let last_cohort: i32;

        let bounds = self.get_bounds();
        if bounds.is_none() { return vecs; }
        let bounds = bounds.unwrap();

        let (f, l, fg, lg) = bounds;
        { first_ym = f; last_ym = l; first_cohort = fg; last_cohort = lg; }

        // Pad out so all months are present in first year. This
        // makes it easier to align the histogram in plots.

        let mut ym = first_ym;
        if ym.month != NO_MONTH { ym.month = 0; }

        while ym <= last_ym
        {
            let mut gens_vec: Vec<(i32, i32)> = Vec::new();
            let sum: i32 =
                if self.bins.contains_key(&ym) { self.bins[&ym].iter().map(|(_, x)| x).sum() }
                else { 0 };

            gens_vec.push((NO_COHORT, sum));

            let mut g = first_cohort;
            while g <= last_cohort
            {
                let value = self.get_value(ym, g).unwrap_or(0);
                gens_vec.push((g, value));
                g += 1;
            }

            if !self.get_cohort_name(NO_COHORT).is_empty()
            {
                gens_vec.push((NO_COHORT, self.get_value(ym, NO_COHORT).unwrap_or(0)));
            }

            vecs.push((ym, gens_vec));
            ym = ym.next();
        }

        vecs
    }

    pub fn to_csv(&self) -> String
    {
        let mut keys = String::new();
        let vecs = self.to_vecs();

        // Print keys in first row.

        let bounds = self.get_bounds();
        if let Some((_, _, mut g, gl)) = bounds
        {
            keys += match vecs[0].0.month
            {
                NO_MONTH => "Year|Sum",
                _ => "Year|Month|Sum"
            };

            while g <= gl
            {
                keys += &format!("|{}", self.get_cohort_name(g));
                g += 1;
            }

            if !self.get_cohort_name(NO_COHORT).is_empty()
            {
                keys += &format!("|{}", self.get_cohort_name(NO_COHORT));
            }

            keys += "\n";
        }

        keys + &vecs.iter()
            .map(|(ym, gens)|
                 if ym.month == NO_MONTH { format!("{}|", ym.year) }
                 else { format!("{}|{}|", ym.year, ym.month) }
                 + &gens.iter()
                     .map(|(_, value)| format!("{}", value))
                     .collect::<Vec<String>>()
                     .join("|"))
            .collect::<Vec<String>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_month_next() {
        assert_eq!(
            YearMonth {
                year: 2020,
                month: NO_MONTH,
            }.next(),
            YearMonth {
                year: 2021,
                month: NO_MONTH,
            },
        );
    }

    #[test]
    fn with_month_next() {
        assert_eq!(
            YearMonth {
                year: 2020,
                month: 0,
            }.next(),
            YearMonth {
                year: 2020,
                month: 1,
            },
        );

        assert_eq!(
            YearMonth {
                year: 2020,
                month: 11,
            }.next(),
            YearMonth {
                year: 2021,
                month: 0,
            },
        );
    }
}
