# Discord Backup Util

A tool that backs up whatever you need to Discord.

## Setup

- Install
> `$ cargo install discord-backup-util`
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

## Windows

We never needed to use this on Windows, so we don't guarantee that any Windows build will even launch.

## Features policy

If a feature is not too insane, feel free to submit a [feature request](https://github.com/5GameMaker/discord-backup-util/issues/new?assignees=&labels=enhancement&projects=&template=feature_request.md&title=feature%3A+This+one%21). If you can actually work on a feautre, fork this repo and then submit a PR, although it'd be nice to open a FR first to see if your work is going to be accepted into the project.

## Contributing

Submit all PRs to `master` branch. PRs to `stable` will not be accepted.
