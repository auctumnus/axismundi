# axismundi

conlanging community platform

## Development

This is written in Rust, with a simple frontend using Typescript.

### Backend

Run the backend with `just run` (or inspect the `justfile` any other time a `just` command is needed).
This assumes a database is running, which can be started with `just db`.

#### Services

We use:
- a PostgreSQL database
- an S3 compatible storage service (MinIO, or Cloudflare R2, etc ...)
- [Resend](https://resend.com/) for email sending (though it should be easy to swap out for another service)

#### Layout

The backend has a number of _models_, which are the main entities in the system, such as `User`, `Lang`, etc.
Then, on top, we have the controllers, which handle the HTTP requests and responses.
These abstract over the internal model handling, either through HTML or JSON.

### Frontend

The frontend should be built with `just build`, which uses SWC to compile the Typescript files,
and processes the CSS files with LightningCSS.