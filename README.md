# Yellhole

## A Hole To Yell In

A lightweight tumblelog which can run on e.g. [fly.io](https://fly.io). All persistent data is
stored in a single directory which can be a mounted persistent volume.

## Features

* Run as a single node. Use a CDN if you're popular.
* All data is stored in a single directory.
* Simple single-user registration/login with Passkeys.
* Simple mobile-friendly interface.
* Write posts in Markdown.
* Upload images of any format (including HEIC), it converts them to WebP.
* Download images via URL, same thing.
* Simple image gallery makes it easy to post images.
* No titles, contents addressable by ID, contents sorted by time.
* Atom feed so your friends can watch.

## TODO

* [ ] Improve testing
  * [x] Test note creation
  * [ ] Test image uploads (mock out IM)
  * [ ] Test image downloads (mock out IM)
  * [ ] Test passkey registration
  * [ ] Test passkey authentication
