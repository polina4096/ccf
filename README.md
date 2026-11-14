# ccf

Search your local Claude Code conversations from the terminal.

Type to filter, browse with a live transcript preview, press enter to resume
the chat in its directory. Chats are found by title, project, branch, or any
words that appear anywhere in the conversation.

```
ccf            open the picker
ccf --print    print the chosen session id instead of resuming it
ccf --reindex  rebuild the index from scratch
ccf --verify   rebuild the index and report any records that could not be read
ccf --open-config  edit the configuration file
```

## Keys

| key | action |
| --- | --- |
| enter | resume the chat (`claude --resume`) in its directory |
| tab | move focus to the preview, where ↑ ↓ pgup pgdn home end scroll it; tab or esc returns |
| shift-tab | toggle the preview |
| click | focus the pane under the pointer; on a chat, select it |
| wheel | scroll the pane under the pointer |
| ↑ ↓ / ctrl-p ctrl-n | move |
| ← → | move in the search, by word with alt or ctrl |
| ctrl-d ctrl-e | scroll the preview, ctrl-g jumps to its end |
| ctrl-y | copy the resume command |
| ctrl-r | rescan |
| ctrl-t | show only chats from the current directory and below |
| esc | clear the search, then quit |

Search takes fzf-style operators: `'word` for exact, `^word` for prefix,
`!word` to exclude.

## Install

```
cargo install ccf
```

or

```
cargo install --path .
```

The first launch indexes every session in `~/.claude/projects`. The index
persists in `~/Library/Caches/ccf` on macOS or `~/.cache/ccf` on Linux
(safe to delete, rebuilt on the next launch), and only changed chats are
re-read afterwards, in the background.

Settings live in `~/.config/ccf/config.toml`, written with commented defaults
on first launch.

## License

[Unlicense](LICENSE).
