# General Plan

A lightweight tumblelog which can run on Google Cloud Run. All persistent data is either
configuration or stored in a single directory which can be a FUSE/GCE mount when deployed.

## Features

* All persistence is SQLite.
* Run as a single node. Use a CDN if you're popular.
* Simple single-user registration/login with Webauthn only
* Simple mobile-friendly interface
* Handles posting of different types of content
  * Notes (Markdown text)
  * Links (URL w/ title and description)
  * Embeds (oEmbed URL e.g. YouTube or Twitter)
  * Images (must have easy upload of HEIC images from iOS)
  * Code? Gists?
* No titles, contents addressable by ID, contents sorted by time
  * Main index page has X most recent entries
  * Archives roll up by month (week?)

## TODO

* [x] Get a hello world Axum server going
* [x] Get a basic DB setup working
* [x] Add links
* [x] Make an actual combined feed HTML template
* [x] Make a publishing UI
* [x] Add `Cache-Control` for feed pages
* [ ] Add images schema
* [ ] Add image uploads w/ resizing
  * write original to `data/images/orig/{id}.{mime-ext}`
  * [shell out to ImageMagick](https://docs.rs/tokio/latest/tokio/process/index.html)
  * transcode to WebP
  * handle JPEG, PNG, WebP, HEIC, GIF, animated GIFs
  * strip metadata (`-strip`)
  * make thumbnail size and feed size
* [ ] Add image uploads via URL
* [ ] Add image serving
  * Liberal use of `Cache-Control`
  * Don't serve original files
* [ ] Add single-shot upload-and-note UI
* [ ] Add single-shot URL-and-note UI
* [ ] Add image gallery w/ click-to-insert UI for notes
* [ ] Add sessions/authentication/credentials
  * [ ] Add credentials schema
  * [ ] Add sessions schema (IP, User-Agent, geo-location)
  * [ ] Add initial PIN to config
  * [ ] Use [Passkeys](https://www.imperialviolet.org/2022/09/22/passkeys.html) after first basic auth
  * [ ] Add DB-backed sessions w/ opaque cookies
  * [ ] Add UI for listing/revoking sessions
