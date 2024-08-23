# Discord Backup Util

A tool that backs up whatever you need to Discord.

## Setup

- Install
> `$ cargo install --git https://github.com/5GameMaker/discord-backup-util.git`
- Create `backup_config`:
> `$ discord-backup-util --setup`
- Adapt
> `$ your-favorite-editor backup_config`
- Launch
> `$ discord-backup-util`

## Things to do after

- Setup a cron job/systemd service to start `discord-backup-util` on boot.
- Password-protect the artifacts (they are being uploaded to Discord of all places after all).
- Rethink your life choices of why are you backing up your infrastructure to Discord.
