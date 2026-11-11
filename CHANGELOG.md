# Changelog

All notable changes to Aloud are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.2.1]

### Fixed

- Reading speed is limited to a safe range everywhere, not only by the slider (#25).
- Too many pronunciation fixes to apply now give a clear message when you add them (#24).
- Turning off graphics acceleration after a failure is saved reliably (#42).
- Chapters saved as Latin-1, Windows-1252 or UTF-16 text show their accents and symbols correctly (#27).
- A chapter listed twice in an EPUB's reading order is added once (#28).
- A voice pack that is already downloading can't be started a second time, and a damaged or oversized download is discarded (#26).
- Removing or clearing one book no longer stops the audio of another (#47).
- Playback no longer stutters when the next part of a chapter is read from disk (#36).
- Opening "Words to check" on a paused book no longer changes its progress (#41).
- Changing a book's chapters, voice or speed responds at once, and "% to redo" fills in a moment later (#33).
- The book page no longer rereads the book's files every second while it renders (#34).
- A book that can't be opened shows an error with a way back to your books, and a failing library shows one message instead of many (#40).
- Quitting with Cmd+Q or from the Dock while a book is being made asks first, and quitting shows its progress (#39).
- Sliders, progress bars and the queue button have names for screen readers, and light-on-light text is easier to read (#48).
- Downloads trust the certificates installed on your computer, and a blocked secure connection gets its own message (#62).
- A voice pack can't be removed while a download is running, and a failed download can't leave Aloud thinking one is still going (#43).
- Audiobook file names stay within the file-name length limit, avoid names Windows reserves, and drop invisible direction characters (#22).
- A finished audiobook is written to disk before it appears, and half-written `.m4b.part` files left by a crash are cleaned up when Aloud starts (#21).
- Changing a book's cover keeps the old one if the new one can't be saved, and covers are decoded once (#23).
- Saved audio that no longer belongs to a book (after a voice or text change), leftover temporary files and half-finished imports are cleaned up (#29).
- Saved voice previews are capped at 50 MB, oldest first, and Settings shows them separately with a "Clear previews" button (#44).
- Moving a pronunciation fix between one book and all books can no longer leave it in both lists (#46).

## [0.1.0]

### Added

- Turn a DRM-free English EPUB into a chaptered M4B audiobook with Kokoro voices, entirely on your computer.
- Voice picker with a spoken preview, reading speed and a spoken title and author introduction.
- Pronunciation dictionary and a "words to check" finder.
- Built-in player.
- Render queue with pause, resume and cancel; finished parts are kept when you stop.
- In-app updates, signed and checked before they are installed.
- Third-party licence notices shipped with the app.

### Fixed

- The Open audiobook button works again (#1).
- A crash while planning, making or exporting a book no longer stops the render queue (#2).
- Books with chapter titles over 255 bytes, or more than 255 chapters, export correctly (#3).
- Aloud checks the audiobooks folder when you choose it, not after the whole render (#20).
- Unreadable books show up in the library, marked as damaged, so you can remove them (#19).
- Errors read as plain sentences instead of raw file or data messages (#37).
- If Aloud can't start, it shows an error screen with a Retry button (#38).
- Playback recovers when the audio device changes or is unplugged (#35).
- A cut-off or empty piece of saved audio is made again instead of ending up as a gap or an error (#16).
- Changing book details while it renders, or at the same moment another change is saved, no longer loses edits (#12).
- A damaged settings or queue file is kept as a `.bak` copy and replaced by defaults, and Aloud tells you (#14).
- Cancelling or pausing during planning or while writing the audiobook now stops right away (#15).
- Pronunciation previews no longer clash with a book that is being planned, and no longer add words to the check list (#13).
- Aloud refuses ids and file paths that don't belong to a book or image (#5).
- Very large or booby-trapped EPUBs are rejected instead of using up your memory (#6).
- An empty `<title/>` in a chapter file no longer swallows the whole chapter (#17).
- A table of contents that points to several places in one file now gives one chapter for each (#18).
