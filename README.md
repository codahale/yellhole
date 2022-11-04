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
* [ ] Get a basic DB setup working
* [ ] Add an `/admin/login` `GET` page
  * [ ] Checks for a `./data/credentials` file
  * [ ] If that doesn't exist, prompts for a WebAuthn registration <https://www.imperialviolet.org/2022/09/22/passkeys.html>
* [ ] Add an `/admin/register` `POST` page
  * [ ] Writes the WebAuthn credentials to `./data/credentials`
