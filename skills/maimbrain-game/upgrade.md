# Upgrading a game to a new kit

A game keeps the SDK it was first built with (its Cargo.lock pins it) until someone moves it. So when you start work on an existing game:

1. **Run `mb upgrade <game dir>` first** (`--json` to read it as data). It says which SDK the game builds with and this kit's version, prints the upgrade notes of every newer release, and checks the game's code and manifest for what applies to it, with file and line.
2. **Do what's Required.** `mb upgrade --apply <game dir>` makes the mechanical fixes (moves Cargo.lock to this kit's SDK, marks the board `Rounds::daily(n)` names `daily = true`); fix the rest by hand, then `mb build` and `cargo test -p <name>`. A game whose SDK is a `path` dependency (in the Maimbrain repo) has no lock to move.
3. **Recommended and Available items are suggestions, not chores.** Ask the owner before adopting anything that changes how the game plays or looks: daily boards, intros and onboarding, walking, hints (`mb upgrade` marks these "ask the owner first"). A tuned game's feel is the owner's ([Continuing a tuned game](SKILL.md#continuing-a-tuned-game)). Sound, story lines, bot reports and `mb art meta` you can go ahead with.

`mb build`, `mb serve` and `mb publish` print a line when a game's SDK is behind the kit, and `mb doctor` says when a newer `mb` is out. This skill updates separately: `/plugin update maimbrain@maimbrain-kit` in Claude Code.
