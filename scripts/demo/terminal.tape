# CLI tour for the README, recorded with vhs (https://github.com/charmbracelet/vhs).
# `npm run screenshots` runs this inside a freshly seeded demo workspace with `lotar`
# on PATH and copies terminal.gif to docs/assets/screenshots/.

Output terminal.gif

Set Shell "bash"
Set FontSize 15
Set Width 1500
Set Height 560
Set Padding 24
Set Theme "Catppuccin Mocha"
Set TypingSpeed 28ms

Hide
Type "export PS1='\[\e[38;5;117m\]atlas\[\e[0m\] $ ' && clear"
Enter
Show

Type `lotar add "Retry failed webhooks with backoff" --type bug -P high`
Sleep 300ms
Enter
Sleep 1.6s

Type "lotar list --status InProgress"
Sleep 300ms
Enter
Sleep 2.2s

Type "clear && lotar scan --dry-run web/src/editor ios"
Sleep 300ms
Enter
Sleep 2.6s

Type "clear && lotar stats authors --since 30d"
Sleep 300ms
Enter
Sleep 2.4s
