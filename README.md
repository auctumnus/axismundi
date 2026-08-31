<p align="center">
  <a href="https://axismundi.app#gh-light-mode-only"><img src="https://github.com/auctumnus/axismundi/blob/main/assets/axismundi-wordmark.svg#gh-light-mode-only" width=333 /></a>
  <a href="https://axismundi.app#gh-dark-mode-only"><img src="https://raw.githubusercontent.com/auctumnus/axismundi/refs/heads/main/assets/axismundi-wordmark-light.svg#gh-dark-mode-only" width=333 /></a>
  <p align="center">a conlanging community and platform</p>
</p>
<p align="center">
  <a href="https://discord.gg/VGzdwcdKzu"><img alt="Discord" src="https://img.shields.io/discord/924860049048883251"></a>
  <a href="https://github.com/auctumnus/axismundi/actions/workflows/check.yml"><img alt="GitHub branch check runs" src="https://img.shields.io/github/check-runs/auctumnus/axismundi/main"></a>
  <a href="https://bsky.app/profile/axismundi.app"><img alt="Bluesky followers" src="https://img.shields.io/bluesky/followers/axismundi.app"></a>
</p>

## Screenshots
<img width="32%" alt="a screenshot of the axismundi homepage, featuring recent activity" src="https://github.com/user-attachments/assets/1434268a-fb32-4cde-94e9-ae3d934c6208" />
<img width="32%" alt="a screenshot of a language's page in axismundi, showing recently-added words and translations" src="https://github.com/user-attachments/assets/f9528cca-00c8-479a-a373-3399849a38f4" />
<img width="32%" alt="a screenshot of a user's profile in axismundi, showing their languages and translatables" src="https://github.com/user-attachments/assets/65937521-ffa1-47e1-8301-cf4594343141" />

## Features

- document your conlangs, their dictionaries, their families...
- collaboration between you and your friends
- modern, responsive design

We have an official instance at [https://axismundi.app](https://axismundi.app). You can join [our discord](https://discord.gg/VGzdwcdKzu) as well.

## Development

Axismundi is written in Rust, with a simple frontend using Typescript (and some React components).

If you want a full dev environment, run `just dev`; this will run all the required Docker containers
and start the app, auto-reloading on changes.

You will need to fill out a `config.json`; an example one is provided in [`./resources/config.json`](./resources/config.json).

### Backend

Run the backend with `just run` (or inspect the `justfile` any other time a `just` command is needed).
This assumes a database is running, which can be started with `just db`.

### API CLI

The repository also builds `axm`, a resource-oriented API client. It supports
words, languages, word classes/categories, definitions, community content, and
language-structure resources. For example, create a dictionary entry with:

```bash
export AXISMUNDI_API_TOKEN='your-token' # preferred over --token: avoids shell history
cargo run --bin axm -- word new --in pas --word "kok'ebe" --def 'to stink' --class v
```

Repeat `--def` for multiple ordered senses and `--category` for multiple word
categories. The same resource supports `list`, `get` (or `read`), `edit`, and
`delete`; for example: `axm word edit --in pas --slug kokebe --lemma 1 --ipa
kɔ.kʼe.be`. Use `axm content --help` for translatables, translations,
quotations, and news; use `axm structure --help` for phonology tables,
sound-change sets, and language-family resources. `AXISMUNDI_API_URL`
overrides the default `https://axismundi.app/api`; `AXISMUNDI_WEB_URL` sets the
website base URL used by word-list links (otherwise it is inferred by removing
`/api`). Interactive word searches render as compact, colored listings with
definition previews, relative timestamps, and clickable word links when the
terminal advertises OSC 8 hyperlink support; other JSON responses are
pretty-printed. When piped or captured,
raw response bytes are written instead. Pass `--json` to request raw output
explicitly. HTTP status and errors go to stderr, which makes the command
convenient in scripts.

For tmux, enable forwarding in `~/.tmux.conf` with
`set -as terminal-features ',*:hyperlinks'`. `axm` reads tmux's configured
terminal features when its ordinary terminal detection cannot identify the
outer terminal.
Redirects are not followed unless `--follow` is passed. The client retries
rate-limited reads with bounded backoff; write retries require the explicit
`--retry-writes` opt-in. It will not send a token over remote plain HTTP
without `--allow-insecure-http`.

Run `cargo run --bin axm -- --help` for the complete option list and
see [the API tutorial](docs/API-tutorials.md) for a parent-to-daughter word
workflow.

#### CLI configuration

`axm` reads `$XDG_CONFIG_HOME/axm/config.json`, falling back to
`~/.config/axm/config.json`. Copy and adapt
[`resources/axm-config.json`](resources/axm-config.json):

```json
{
  "api_url": "https://axismundi.app/api",
  "web_url": "https://axismundi.app",
  "token_file": "/path/to/axm-token",
  "default_language": "pas"
}
```

Relative `token_file` paths are relative to that configuration file.
`--base-url`, `--token`, `--token-file`, and their existing environment
variables override it; `--in` overrides `default_language`. Set `AXM_CONFIG`
to use a configuration file at another path.

#### Services

We use:
- a PostgreSQL database
- an S3 compatible storage service (MinIO, or Cloudflare R2, etc ...)
- [Resend](https://resend.com/) for email sending (though it should be easy to swap out for another service)

#### Layout

The backend has a number of _models_, which are the main entities in the system, such as `User`, `Language`, etc.
Then, on top, we have the controllers, which handle the HTTP requests and responses.
These abstract over the internal model handling, either through HTML or JSON.

### Frontend

The frontend should be built with `just build`, which uses SWC to compile the Typescript files,
and processes the CSS files with LightningCSS.

### Testing

We use integration testing with real Postgres and real MinIO, and a mocked email module. The justfile
will just set this up for you so long as you have Docker (or some compatible runtime).

## License

Axismundi is distributed under the [Non-violent Public License](./LICENSE.md).
The source for the NVPL can be found at [its repo](https://git.pixie.town/thufie/npl-builder).
