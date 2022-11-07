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
* [x] Add images schema
* [x] Add image uploads w/ resizing
* [x] Add image serving
  * [x] Liberal use of `Cache-Control`
  * [ ] Non-funky 404 errors
  * [x] Don't serve original files
* [ ] Test images
  * [ ] JPEG
  * [ ] PNG
  * [ ] WebP
  * [ ] HEIC
  * [ ] GIF
  * [ ] Animated GIF
* [ ] Add image uploads via URL
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
