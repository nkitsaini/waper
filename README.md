# Waper

Waper is a CLI tool to scrape html websites. Here is a simple usage
```
waper --seed-links "https://example.com/" --whitelist "https://example.com/.*" --whitelist "https://www.iana.org/domains/example" 
```
This will scrape "https://example.com/" and save the html for each link found in a sqlite db with name `waper_out.sqlite`.

## Installation
```
cargo install waper
```

## CLI Usage
```
Program to scrape websites and save html to a sqlite file. Example: waper --whitelist "https://example.com/.*" --whitelist "https://www.iana.org/domains/example" -s "https://example.com/"

Usage: waper [OPTIONS]

Options:
  -w, --whitelist <WHITELIST>      whitelist regexes: only these urls will be scanned other then seeds
  -b, --blacklist <BLACKLIST>      blacklist regexes: these urls will never be scanned By default nothing will be blacklisted [default: a^]
  -s, --seed-links <SEED_LINKS>    Links to start with
  -o, --output-file <OUTPUT_FILE>  Sqlite output file [default: waper_out.sqlite]
  -v, --verbose                    Should verbose (debug) output
  -h, --help                       Print help
  -V, --version                    Print version
```

## Querying data

Data is stored in sqlite db with schema defined in [./sqls/INIT.sql](./sqls/INIT.sql). There are three tables
1. `results`: Stores the content of all the request for which a response was recieved
2. `errors`: Stores the error message of all the cases where the request could not be completed
3. `links`: Stores the urls of both visited or unvisited links
  

Result can be queried using any sqlite client. Example using [sqlite cli](https://www.sqlite.org/cli.html):
```bash
$ sqlite3 waper_out.sqlite 'select url, time, length(html) from results'
https://example.com/|2023-05-07 06:47:33|1256
https://www.iana.org/domains/example|2023-05-07 06:47:39|80
```
  
For beautiful output you can modify sqlite3 settings:
```bash
$ sqlite3 waper_out.sqlite '.headers on' '.mode column' 'select url, time, length(html) from results'
url                                   time                 length(html)
------------------------------------  -------------------  ------------
https://example.com/                  2023-05-07 06:47:33  1256
https://www.iana.org/domains/example  2023-05-07 06:47:39  80
```
  
To quickly search through all the urls you can use [fzf](https://github.com/junegunn/fzf):
```bash
sqlite3 waper_out.sqlite 'select url from links' | fzf
```

## Planned improvements
- [ ] Allow users to specify priority for urls, so some urls can be scraped before others.
- [ ] Support complex rate-limits
- [ ] Allow continuation of previously stopped scraping.
- [ ] Explicitly handling redirect.
- [ ] Allow users to modify part of request (like user-agent).
- [ ] Improve storage efficiency by compressing/de-duping the html
- [ ] Provide more visibility into how many urls are queued, at which rate are they getting processed etc.

## Feedback
If you find any bugs or have any feature suggestions please file [an issue](https://github.com/nkitsaini/waper/issues) on github.

