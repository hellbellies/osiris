-- Issues in a status the user watches (Jira settings: "Also show"), whoever they are
-- assigned to. Recomputed on every sync; watched issues are listed and kept like
-- assigned ones.
ALTER TABLE jira_issues ADD COLUMN watched INTEGER NOT NULL DEFAULT 0;
