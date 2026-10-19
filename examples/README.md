# Field Station

The Method Production Suite's shared sample: an invented feature screenplay, about 80 pages and
90 scenes, written for the suite. Storyteller writes it, Method Schedule breaks it down, Method
Budget budgets that schedule, and Method Reader reads it, so the apps' pictures and hand-offs tell
one story. Everything in it is invented: the title, the people, the places and every line.

| File | What it is |
|---|---|
| `field-station.osf` | The master, in Open Screenplay Format, Storyteller's standard |
| `field-station.fountain` | The same script as Fountain, written from the master |
| `field-station.fdx` | The same script as `.fdx`, written from the master, for the apps that import it |
| `field-station.pdf` | The same script printed, for reading |

All four read back to the same script (80 pages, 90 scenes). Edit the master and write the other
three from it:

```bash
cargo run --release -p storyteller-cli -- convert examples/field-station.osf examples/field-station.fountain
cargo run --release -p storyteller-cli -- convert examples/field-station.osf examples/field-station.fdx
cargo run --release -p storyteller-cli -- pdf examples/field-station.osf examples/field-station.pdf
```

Its shape follows the suite's shared shoot (18 days): Cold Spring field station (the hut, the lab,
the bunkhouse, the yard, the radio mast, the ridge, the lower station and the base radio room),
the Hollins Ridge fire road (including a drive a second unit shoots), and the Kettle Lake
boathouse (the company move). Eight speaking parts: DELACROIX, MARA, TEO, IVERSEN, RANGER,
BOATHOUSE KEEPER, PILOT and DISPATCHER. Scenes 1 to 11 are the opening Method Schedule and Method
Reader have used since the start, unchanged.

It shows what Storyteller writes: a title page, act sections and synopses, scene numbers, dual
dialogue, a shot, speeches in more than one paragraph, and production notes.
