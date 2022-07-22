# Fornalder

Fornalder ("Bygone Age") is a small utility that can be used to ingest
commit data from collections of git repositories and visualize it in
various ways.

It was used to generate the graphs in [this blog post](https://hpjansson.org/blag/2020/12/16/on-the-graying-of-gnome/). It's made to work with data going back a long time (a decade or two); with shorter time spans the output will probably look quite bad.

## Building

[Clone](https://github.com/git-guides/git-clone) the project repository and make sure you have the Rust prerequisites installed, then:

```sh
$ cargo build
```

## Using

You need a fairly recent version of Gnuplot to generate plots. Make sure
it is installed.

Clone the repositories of interest to a local directory, then ingest them.
This can be run multiple times to add to or update the database:

```sh
$ target/debug/fornalder --meta projects/project-meta.json \
                         ingest db.sqlite repo-1.git repo-2.git ...
```

When the database has been created, generate one or more plots, e.g:

```sh
$ target/debug/fornalder --meta projects/project-meta.json \
                         plot db.sqlite \
                         --cohort firstyear \
                         --interval year \
                         --unit authors \
                         graph.png
```

If something looks odd in the result, you can also explore the database directly.

```sh
$ sqlite3 db.sqlite
sqlite> .tables
authors      raw_commits
sqlite> .schema authors
CREATE TABLE authors(
  author_name TEXT,
  first_time,
  first_year,
  last_time,
  last_year,
  active_time,
  n_commits,
  n_changes
);
sqlite> SELECT author_name, first_year FROM authors ORDER BY first_year;
[...]
```

Guide to arguments:

```
--meta <meta>
    Optional. Project metadata to use. See projects/ for examples.

--cohort < domain | firstyear | prefix | repo | suffix >
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


## Git cloning tips

We support ingestion from both bare and non-bare repositories:

    git clone https://git.example.com
    git clone --bare https://git.example.com

Another option that can massively reduce data transfer is `--filter=blob:none`, which will only clone commit metadata, not the files themselves.

    git clone --bare --filter=blob:none https://git.example.com

Note that this is not supported by all git servers (cloning may fail). Using this mode also prevents the use of the `--unit changes` option (counting the number of changed lines), or in general inspecting `--stat` output from the commit database.
