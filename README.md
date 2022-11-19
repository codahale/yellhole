# Yellhole

![A hole to yell in.](yellhole.webp)

## A Hole To Yell In

Yellhole is a lightweight tumblelog which can run on e.g. [fly.io](https://fly.io) for cheap.

## Features

* Runs on a single node. Use a CDN if you're popular.
* All data is stored in a single directory.
* Simple single-user registration/login with Passkeys.
* Simple mobile-friendly interface.
* Write posts in Markdown.
* Upload images of any format (including HEIC), it converts them to WebP.
* Download images via URL, same thing.
* Simple image gallery makes it easy to post images.
* No titles, contents addressable by ID, contents sorted by time.
* Atom feed so your friends can watch.

## Installation

```shell
cargo install --locked yellhole
```

Requires SQLite and a TLS stack as build dependencies.

Requires ImageMagick as system dependency (specifically, `convert` must be in `$PATH`).

## Operation

See `Dockerfile` for packaging example. See `fly.toml` for deployment example.

## Shitposting

1. Get Yellhole running somewhere.
2. Go to `/register` and register a Passkey.
3. Log in with your Passkey and go to `/admin/new`.
4. Shitpost.
5. Go to `/` and admire your work.

## License

Copyright © 2022 Coda Hale

Distributed under the Affero General Public License (v3 or later).
