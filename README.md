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

<img width="33%" alt="a screenshot of the axismundi homepage, featuring recent activity" src="https://github.com/user-attachments/assets/3c175619-cb88-4888-8bb4-42e630f47eb0" />
<img width="33%" alt="a screenshot of a language's page in axismundi, showing recently-added words and translations" src="https://github.com/user-attachments/assets/36c9fd85-d3fb-4069-a0ac-d23d92d514cf" />
<img width="33%" alt="a screenshot of a user's profile in axismundi, showing their languages and translatables" src="https://github.com/user-attachments/assets/17a657fb-8eac-4391-bfbd-11f5ec351f57" />

## Features

- document your conlangs, their dictionaries, their families...
- collaboration between you and your friends
- modern, responsive design

to participate in beta tests for our official instance, join [our discord](https://discord.gg/VGzdwcdKzu)

## Development

Axismundi is written in Rust, with a simple frontend using Typescript (and some React components).

If you want a full dev environment, run `just dev`; this will run all the required Docker containers
and start the app, auto-reloading on changes.

### Backend

Run the backend with `just run` (or inspect the `justfile` any other time a `just` command is needed).
This assumes a database is running, which can be started with `just db`.

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
