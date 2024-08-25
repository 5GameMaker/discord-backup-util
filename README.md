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

## Building for 32-bit platforms

We support building `discord-backup-util` down to i586, although build might fail due
to some C packages failing to compile.

If build fails due to dependencies, add `--no-default-features --features minreq` to command line
(This may take longer to compile as for `minreq` we use bundled OpenSSL instead of RusTLS) (Not all
targets can be fixed this way).
