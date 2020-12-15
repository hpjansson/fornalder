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

/* ------- *
 * Plotter *
 * ------- */

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tempfile::NamedTempFile;
use crate::cohorthist::CohortHist;
use crate::errors::*;
use crate::projectmeta::ProjectMeta;

const GNUPLOT_COHORTS_COMMON: &str = "
set style line 1 lt 1 lc rgb '#909090';
set style line 2 lt 1 lc rgb '#505050';
set style line 3 lt 1 lc rgb '#a6cee3';
set style line 4 lt 1 lc rgb '#1f78b4';
set style line 5 lt 1 lc rgb '#c2a5cf';
set style line 6 lt 1 lc rgb '#9970ab';
set style line 7 lt 1 lc rgb '#b2df8a';
set style line 8 lt 1 lc rgb '#33a02c';
set style line 9 lt 1 lc rgb '#fb9a99';
set style line 10 lt 1 lc rgb '#e31a1c';
set style line 11 lt 1 lc rgb '#fdbf6f';
set style line 12 lt 1 lc rgb '#ff7f00';
set style line 13 lt 1 lc rgb '#6b3d15';
set style line 14 lt 1 lc rgb '#bf812d';
set style line 15 lt 1 lc rgb '#458e81';
set style line 16 lt 1 lc rgb '#34c0b5';
set style line 17 lt 1 lc rgb '#40004b';
set style line 18 lt 1 lc rgb '#762a83';
set style line 19 lt 1 lc rgb '#00441b';
set style line 20 lt 1 lc rgb '#1b7837';
set style line 21 lt 1 lc rgb '#a50026';
set style line 22 lt 1 lc rgb '#d73027';
set style line 23 lt 1 lc rgb '#053061';
set style line 24 lt 1 lc rgb '#2166ac';
set style line 25 lt 1 lc rgb '#40004b';
set style line 26 lt 1 lc rgb '#762a83';
# -- Repeat --
set style line 27 lt 1 lc rgb '#909090';
set style line 28 lt 1 lc rgb '#505050';
set style line 29 lt 1 lc rgb '#a6cee3';
set style line 30 lt 1 lc rgb '#1f78b4';
set style line 31 lt 1 lc rgb '#c2a5cf';
set style line 32 lt 1 lc rgb '#9970ab';
set style line 33 lt 1 lc rgb '#b2df8a';
set style line 34 lt 1 lc rgb '#33a02c';
set style line 35 lt 1 lc rgb '#fb9a99';
set style line 36 lt 1 lc rgb '#e31a1c';
set style line 37 lt 1 lc rgb '#fdbf6f';
set style line 38 lt 1 lc rgb '#ff7f00';
set style line 39 lt 1 lc rgb '#6b3d15';
set style line 40 lt 1 lc rgb '#bf812d';
set style line 41 lt 1 lc rgb '#458e81';
set style line 42 lt 1 lc rgb '#34c0b5';
set style line 43 lt 1 lc rgb '#40004b';
set style line 44 lt 1 lc rgb '#762a83';
set style line 45 lt 1 lc rgb '#00441b';

set terminal pngcairo size 2560,1200 enhanced background rgb 'white' font 'Verdana,25';
set datafile separator '|';
set rmargin 1.1;
set tmargin 0.6;
set bmargin 7.0;
set border 3;
set decimalsign locale;
set decimalsign ',';
set format y \"%'.0f\";
set border lw 2;
set style fill solid;
set style line 101 lc rgb \"0x50000000\" dashtype '-' lw 2;
set yrange [] writeback;
set style data histogram;
set style histogram rowstacked;
set xtics scale 0 nomirror offset 0,graph 0.015;
set ytics nomirror;
set key autotitle columnheader;
set key reverse Left horizontal nobox bmargin left width 1.1;
set ytics textcolor rgb \"0xff000000\" scale 0;
";

pub struct Plotter
{
}

impl Plotter
{
    pub fn plot_yearly_cohorts(&self,
                               meta: &ProjectMeta,
                               unit: &String,
                               hist: &CohortHist, out_file: &PathBuf,
                               first_year: Option<i32>, last_year: Option<i32>) -> Result<()>
    {
        let bounds = hist.get_bounds().unwrap();
        let first_year =
            if first_year.is_some() { first_year.unwrap() }
            else if meta.first_year.is_some() { meta.first_year.unwrap() }
            else { bounds.0.year };
        let last_year =
            if last_year.is_some() { last_year.unwrap() }
            else if meta.last_year.is_some() { meta.last_year.unwrap() }
            else if bounds.0.year == bounds.1.year { bounds.1.year }
            else { bounds.1.year - 1 };
        let markers = meta.markers_to_gnuplot();
        let gnuplot_cmd = format!("
            {}
            set style line {} lt 1 lc rgb '#ffffd0';
$data << EOD
{}
EOD
            set output \"{}\";
            set ylabel \"{}\";
            set xrange [{}:{}];
            set multiplot;
            plot for [i=3:{}] '$data' using i:xtic(stringcolumn(1)) ls i-2 title columnheader(i);
            unset key;
            set style data histep;
            set xtics textcolor rgb \"0xff000000\" scale 1 0.5,1;
            set ytics textcolor rgb \"0x00000000\" scale default;
            set grid xtics ytics front linestyle 101;
            set yrange restore;
            set style textbox opaque noborder;
            {}
            {}
            plot '$data' using 2 lc rgb 'black' lw 2 notitle;
            unset multiplot;
            ",
            GNUPLOT_COHORTS_COMMON,
            hist.get_n_bins() + 1,
            &hist.to_csv(),
            out_file.to_string_lossy().into_owned(),
            unit,
            (first_year - bounds.0.year) as f32 - 0.5,
            (last_year - bounds.0.year) as f32 + 0.5,
            hist.get_n_bins() + 3,
            &markers.0,
            if markers.1 > 0
            {
                format!("
                    set for [i=0:{}:1] label left markers[int(i)*4+4] \
                        at ((markers[int(i)*4+1]+{})*12+(markers[int(i)*4+2]-1))/12.0-(1.1/2.0), \
                           (0.977-0.05*markers[int(i)*4+3])*GPVAL_Y_MAX \
                           front tc ls 0 boxed;
                    ",
                    markers.1 - 1,
                    - bounds.0.year)
            }
            else
            {
                "".to_string()
            }
        );

        let mut file = NamedTempFile::new().chain_err(|| "Could not write gnuplot script")?;
        writeln!(file, "{}", gnuplot_cmd).chain_err(|| "Could not write gnuplot script")?;

        // println!("{}", gnuplot_cmd);

        let output = Command::new("gnuplot")
            .arg(file.path())
            .output()
            .chain_err(|| "Failed to execute gnuplot")?;

        match output.status.success()
        {
            false => { Err("Gnuplot reported error".into()) },
            true => { Ok(()) }
        }
    }

    pub fn plot_monthly_cohorts(&self,
                                meta: &ProjectMeta,
                                unit: &String,
                                hist: &CohortHist, out_file: &PathBuf,
                                first_year: Option<i32>, last_year: Option<i32>) -> Result<()>
    {
        let bounds = hist.get_bounds().unwrap();
        let first_year =
            if first_year.is_some() { first_year.unwrap() }
            else if meta.first_year.is_some() { meta.first_year.unwrap() }
            else { bounds.0.year };
        let last_year =
            if last_year.is_some() { last_year.unwrap() }
            else if meta.last_year.is_some() { meta.last_year.unwrap() }
            else { bounds.1.year };
        let markers = meta.markers_to_gnuplot();
        let gnuplot_cmd = format!("
            {}
            set style line {} lt 1 lc rgb '#ffffd0';
$data << EOD
{}
EOD
            set output \"{}\";
            set ylabel \"{}\";
            set xrange [{}:{}];
            set multiplot;
            plot for [i=4:{}] '$data' using i:xtic($2==\"06\" \
                ? stringcolumn(1) : \"\") ls i-3 title columnheader(i);
            unset key;
            set style data histep;
            set xtics scale 1 11.5,12 textcolor black;
            set xtics textcolor rgb \"0xff000000\";
            set ytics textcolor rgb \"0x00000000\" scale default;
            set grid xtics ytics front linestyle 101;
            set yrange restore;
            set style textbox opaque noborder;
            {}
            {}
            plot '$data' using 3 lc rgb 'black' lw 2 notitle;
            unset multiplot;
            ",
            GNUPLOT_COHORTS_COMMON,
            hist.get_n_bins() + 1,
            &hist.to_csv(),
            out_file.to_string_lossy().into_owned(),
            unit,
            ((first_year - bounds.0.year) * 12) as f32 - 0.5,
            ((last_year - bounds.0.year) * 12 + 12) as f32 - 0.5,
            hist.get_n_bins() + 4,
            &markers.0,
            if markers.1 > 0
            {
                format!("
                    set for [i=0:{}:1] label left markers[int(i)*4+4] \
                        at ((markers[int(i)*4+1]+{})*12+(markers[int(i)*4+2]))-(2.5), \
                           (0.977-0.05*markers[int(i)*4+3])*GPVAL_Y_MAX \
                           front tc ls 0 boxed;
                    ",
                    markers.1 - 1,
                    - bounds.0.year)
            }
            else
            {
                "".to_string()
            }
        );

        let mut file = NamedTempFile::new().chain_err(|| "Could not write gnuplot script")?;
        writeln!(file, "{}", gnuplot_cmd).chain_err(|| "Could not write gnuplot script")?;

        // println!("{}", gnuplot_cmd);

        let output = Command::new("gnuplot")
            .arg(file.path())
            .output()
            .chain_err(|| "Failed to execute gnuplot")?;

        match output.status.success()
        {
            false => { Err("Gnuplot reported error".into()) },
            true => { Ok(()) }
        }
    }
}
