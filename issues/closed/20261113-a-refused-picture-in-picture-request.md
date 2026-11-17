# A refused picture-in-picture request has already left the current float

**Status:** Closed
**Resolution:** The platform's picture-in-picture request ends the current floats only once its new host exists, so a refusal leaves them showing.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`PictureInPicture.request` ends the image's current float, and then `current`, before it knows the new request can run. On iOS a window request is refused after that end. A video request ends `current` before the video-module check and before `exports.create`. The refusal then sends `pictureinpictureleave` and `pictureinpictureerror`, and the picture that was showing is gone.

## Where

`host/apple/Sources/ExactKit/PictureInPicture.swift`, `request`.

## How to reproduce

Float an image, then request a window on iOS, or request video picture-in-picture in a build with no video module, or where `exact_pip_create` returns null. The current float ends and the image hears an error.

## Direction

Refuse first. End the current float only once the new host exists.
