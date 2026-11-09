# osiris

Desktop app built with Tauri v2, Vue 3, TypeScript, Nuxt UI (Tailwind CSS) and Pinia.

## Prerequisites

- [Rust](https://rustup.rs) (stable)
- Node.js and [pnpm](https://pnpm.io)
- Platform deps from the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/)

## Development

```sh
pnpm install
pnpm tauri dev
```

## Build

```sh
pnpm tauri build
```

The bundles end up in `src-tauri/target/release/bundle/`.

### Install on macOS

```sh
pnpm install:mac
```

Builds a release `osiris.app` (no DMG) and replaces `/Applications/osiris.app` with it. Quit osiris first; a running copy keeps the old version until restarted. The app is not code-signed, but macOS opens builds made on the same machine without complaint.

The installed app and `pnpm tauri dev` share the same identifier, so they use the same database, settings and keychain tokens.

## Connecting Jira and GitHub

osiris only reads from Jira and GitHub. Tokens are stored in the operating system's keychain (macOS Keychain, Windows Credential Manager, Secret Service on Linux), never in plain files, and are not readable from the app's UI. Enter them under **Settings**, click **Test connection**, then **Save**.

### Jira (Atlassian Cloud)

osiris signs in with your Atlassian account email plus an API token.

1. Open <https://id.atlassian.com/manage-profile/security/api-tokens> (Atlassian account → **Security** → **API tokens**).
2. Click **Create API token** — the plain one, **not** "Create API token with scopes". Scoped tokens only work through Atlassian's API gateway, not with your site URL.
3. Give it a name (e.g. `osiris`), pick an expiry (Atlassian allows at most one year) and copy the token. It is shown only once.
4. In osiris under **Settings → Jira**, enter:
   - **Site URL**: e.g. `https://your-team.atlassian.net`
   - **Email**: the email of the Atlassian account that created the token
   - **Projects**: comma-separated project keys, e.g. `FOO, BAR` (the `FOO` in `FOO-123`)
   - **API token**: the token from step 3

The token has the same permissions as your account, so osiris sees exactly the issues you can see in Jira. Self-hosted Jira (Server/Data Center) is not supported yet.

### GitHub

osiris uses a personal access token (PAT) to find open pull requests you authored or were asked to review, plus the other open pull requests in the same repositories. Create it in **your own** account settings (profile picture → **Settings** → **Developer settings** → **Personal access tokens**). There are two kinds; pick the one that matches where your pull requests live.

#### Option 1: fine-grained token — recommended if everything is in one organization

Read-only and limited to that organization.

1. Open <https://github.com/settings/personal-access-tokens/new>.
2. **Resource owner**: choose the **organization**, not your username. If it is not listed, the organization does not allow fine-grained tokens; use option 2.
3. **Repository access**: **All repositories**.
4. **Permissions** → Repository permissions → **Pull requests: Read-only** (Metadata: Read-only is added automatically).
5. Set an expiry and generate the token.
6. Many organizations require an admin to approve fine-grained tokens. Until then the token shows as *pending* and cannot see the organization's repositories.

A fine-grained token covers exactly one owner: pull requests in other organizations or your personal repositories will not show up, and osiris currently stores one GitHub token.

If review requests to a **team** you are in do not appear (only personal ones do), also grant **Organization permissions → Members: Read-only**.

#### Option 2: classic token — if your pull requests span several organizations

1. Open <https://github.com/settings/tokens/new> (**Tokens (classic)** → **Generate new token (classic)**).
2. Tick the **`repo`** scope. GitHub has no read-only variant for private repositories, so this token could also write — osiris only reads, but it is broader than option 1.
3. Generate the token.
4. If your organization uses SAML single sign-on: next to the token, click **Configure SSO** and **Authorize** it for the organization. Without this, that organization's pull requests silently do not appear.

A classic token sees everything your account can see, unless an organization has blocked classic tokens.

#### In osiris

Under **Settings → GitHub**, paste the token and click **Test connection** — it should show your GitHub login. Optionally limit the search with **Repositories**: comma-separated `owner/repo` entries, or `owner` for everything a user or organization owns. Leave it empty to search all repositories the token can see. Then click **Fetch** on the **Pull requests** page.

**Other open pull requests** lists everyone else's open pull requests in the configured **Repositories**. While that field is empty, it covers the repositories your own and review-requested pull requests are in.

Jira issues show the pull requests that mention their key (e.g. `FOO-123`) in the title or description, whether open, merged or closed, as badges that open the pull request on GitHub. osiris finds them with GitHub search on **Fetch** in the same repositories; Jira's own development information is not used. To keep **Fetch** quick, issues in a done status are searched one last time and then keep the pull requests found so far (their state still updates); a pull request that first mentions an issue after it is done does not show up. Changing **Repositories** searches done issues once more.

GitHub Enterprise Server is not supported yet.

See [CLAUDE.md](CLAUDE.md) for architecture and conventions.
