# Fornalder

Fornalder ("Bygone Age") is a small utility that can be used to ingest
commit data from collections of git repositories and visualize it in
various ways.

It's made to work with data going back a long time (a decade or two).
With shorter time spans the output will probably look quite bad.

## Building

Make sure you have the Rust prerequisites installed, then:

```sh
$ cargo build
```

## Using

You need a fairly recent version of Gnuplot to generate plots. Make sure
it is installed.

Clone the repositories of interest to a local directory, then ingest them.
This can be run multiple times to add to or update the database:

```sh
$ target/debug/fornalder --meta project-meta.json \
                         ingest db.sqlite repo-1.git repo-2.git ...
```

When the database has been created, generate one or more plots, e.g:

```sh
$ target/debug/fornalder --meta project-meta.json \
                         plot db.sqlite \
                         --cohort firstyear \
                         --interval year \
                         --unit authors
```

Guide to arguments:

```
--meta <meta>
    Optional. Project metadata to use. See projects/ for examples.

--cohort < firstyear | domain >
    Optional. How to split the data into cohorts.

--interval < year | month >
    Optional. Time interval of each histogram bin.

--unit < authors | changes | commits >
    Optional. What's being measured -- active authors, number of lines
    changed, or commit count.

--from year
    Optional. First year to plot.

--to year
    Optional. Last year to plot.
```
