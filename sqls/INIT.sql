CREATE TABLE  IF NOT EXISTS scrape_results (
  url TEXT UNIQUE ON CONFLICT REPLACE,
  html TEXT,
  time TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_url ON scrape_results(url);
CREATE INDEX IF NOT EXISTS idx_time ON scrape_results(time);
