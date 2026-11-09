-- Jira issues whose mentioning pull requests were searched, with the issue's status
-- category (new, indeterminate, done) and the configured repository scope at that time.
-- A done issue searched while done in the same scope keeps its links without searching
-- again (see services/pull_requests.rs).
CREATE TABLE jira_issue_pull_request_searches (
    issue_id TEXT PRIMARY KEY REFERENCES jira_issues (id) ON DELETE CASCADE,
    category TEXT NOT NULL,
    scope    TEXT NOT NULL
);

-- Earlier syncs searched every issue: count the done ones as searched, so the first sync
-- after this migration doesn't search them all again. Assumes no configured repositories;
-- with some, they are searched once more.
INSERT INTO jira_issue_pull_request_searches (issue_id, category, scope)
SELECT i.id, s.category, ''
FROM jira_issues i
JOIN jira_project_statuses s ON s.project_key = i.project_key AND s.status_id = i.status_id
WHERE s.category = 'done' AND EXISTS (SELECT 1 FROM github_sync_state);
