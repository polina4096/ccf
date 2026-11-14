# cff

Search your local Claude Code conversations from the terminal.

Type to filter, browse with a live transcript preview, press enter to resume
the chat in its directory. Chats are found by title, project, branch, or any
words that appear anywhere in the conversation.

```
cff            open the picker
cff --print    print the chosen session id instead of resuming it
cff --reindex  rebuild the index from scratch
cff --verify   rebuild the index and report any records that could not be read
```

## Keys

| key | action |
| --- | --- |
| enter | resume the chat (`claude --resume`) in its directory |
| tab | toggle the preview |
| ↑ ↓ / ctrl-p ctrl-n | move |
| ← → | move in the search, by word with alt or ctrl |
| ctrl-d ctrl-e | scroll the preview, ctrl-g jumps to its end |
| ctrl-y | copy the resume command |
| ctrl-r | rescan |
| esc | clear the search, then quit |

Search takes fzf-style operators: `'word` for exact, `^word` for prefix,
`!word` to exclude.

## Install

```
cargo install --path .
```

The first launch indexes every session in `~/.claude/projects`. The index
persists in `~/Library/Caches/cff` on macOS or `~/.cache/cff` on Linux
(safe to delete, rebuilt on the next launch), and only changed chats are
re-read afterwards, in the background.

## License

[Unlicense](LICENSE).
