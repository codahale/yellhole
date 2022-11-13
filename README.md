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
  * [x] Non-funky 404 errors
  * [x] Don't serve original files
* [x] Add image gallery w/ click-to-insert UI for notes
* [x] Test images (ImageMagick 7.1.0-52)
  * [x] JPEG
  * [x] PNG
  * [x] WebP
  * [x] Animated WebP
    * produced some very glitchy results
  * [x] HEIC
  * [x] GIF
  * [x] Animated GIF
* [x] Add image uploads via URL
* [x] Add Atom feed
* [ ] Add sessions/authentication/credentials
  * [ ] Add `GET /register` page
    * [ ] Count all passkeys from DB
    * [ ] If any exist and the session isn't authenticated, redirect to `/login`
    * [x] Check for WebAuthn support
    * [x] `fetch` a challenge object from `POST /register/start`
    * [x] Prompt for passkey creation
    * [x] `POST /register/finish` the registration response via `fetch`
    * [x] Redirect to `/login` on `CREATED`
  * [x] Add `POST /register/start` handler
    * [x] Select all passkeys from DB
    * [x] Create challenge response
    * [x] Store the registration state in the session
    * [x] Return the challenge as JSON
  * [x] Add `POST /register/finish` handler
    * [x] Decode the registration response from JSON
    * [x] Read and remove the registration state from the session
    * [x] Verify the registration response
    * [x] Insert the passkey into the DB
    * [x] Return empty `CREATED` response
  * [x] Add `GET /login` page
    * [ ] Count all passkeys from DB
    * [ ] If none exist, redirect to `/register`
    * [x] Check for WebAuthn support
    * [x] `fetch` a challenge object from `POST /login/start`
    * [x] Prompt for passkey authentication
    * [x] `POST /login/finish` the registration response via `fetch`
    * [x] Redirect to `/admin/new` on `CREATED`
  * [x] Add `POST /login/start` handler
    * [x] Select all passkeys from DB
    * [x] Create challenge response
    * [x] Store the authentication state in the session
    * [x] Return the challenge as JSON
  * [x] Add `POST /login/finish` handler
    * [x] Decode the authentication response from JSON
    * [x] Read and remove the authentication state from the session
    * [x] Verify the authentication response
    * [x] Mark the session as authentication
    * [x] Return empty `CREATED` response
* [ ] Add middleware checking for authentication sessions to `/admin/*`
