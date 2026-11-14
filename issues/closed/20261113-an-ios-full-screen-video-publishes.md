# An iOS full-screen video publishes Now Playing while a claimant owns the session

**Status:** Closed
**Resolution:** The iOS full-screen controller takes `avkitPublishes` when made, and `sessionAVKitPublishes` sets it while it shows.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

On iOS, `sessionAVKitPublishes` writes `updatesNowPlayingInfoCenter` only on the inline player controller. `enterFullscreen` builds a new `FullscreenPlayer`, gives it the same `AVPlayer`, and sets the inline controller's player to nil. The new controller is left at AVKit's default, which publishes Now Playing.

A claimant that turned publication off therefore loses the session while the video is full screen: the full-screen controller publishes, and the inline controller no longer holds the player. The macOS full-screen view sets `updatesNowPlayingInfoCenter` to false so the inline view keeps the session.

## Where

`host/apple/videoarm/VideoArm.swift`, `sessionAVKitPublishes` and the iOS `enterFullscreen`. The macOS `FullscreenWindow` sets the flag when it is created.

## How to reproduce

On iOS, play a video while a Now Playing claimant owns the session, so `sessionAVKitPublishes(false)` has run. Enter full screen. The system Now Playing info updates from AVKit for that player.

## Direction

Set the full-screen controller's `updatesNowPlayingInfoCenter` from `avkitPublishes` when it is created, and update that controller again from `sessionAVKitPublishes` while it is on screen.
