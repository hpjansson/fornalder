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
 * Common types *
 * ------------ */

use structopt::clap::arg_enum;
use structopt::StructOpt;

arg_enum!
{
    #[derive(StructOpt, Debug, Copy, Clone)]
    pub enum CohortType
    {
        FirstYear,
        Domain,
        Repo,
        Suffix
    }
}

arg_enum!
{
    #[derive(StructOpt, Debug, Copy, Clone)]
    pub enum UnitType
    {
        Authors,
        Commits,
        Changes
    }
}

arg_enum!
{
    #[derive(StructOpt, Debug, Copy, Clone)]
    pub enum IntervalType
    {
        Month,
        Year
    }
}
