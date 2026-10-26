# Discord sound asset source

The owner authorized RUNE to package these public Discord client sound
effects as its defaults on 2026-10-21. They were obtained without
authentication or access to Discord profiles, caches, credentials, or session
data.

Observed public bundle on 2026-10-21:

- URL: `https://discord.com/assets/web.31d1cd3e6e440749.js`
- SHA-256: `5e48910d7993de9164fc0be6d4762276d5a195946014a9825546df5b04e91424`

Each source URL is `https://discord.com/assets/{asset}`. `SHA256SUMS`
contains the checksum of the packaged byte-identical MP3 file.

| RUNE event | Local file | Public asset |
| --- | --- | --- |
| `join`, `user_join` | `user_join.mp3` | `b135ff6c8e091b43.mp3` |
| `leave`, `user_leave` | `user_leave.mp3` | `7b9a183742515fc2.mp3` |
| `mute` | `mute.mp3` | `2d3b4ba32c34c862.mp3` |
| `unmute` | `unmute.mp3` | `e74c4a06134a20e4.mp3` |
| `deafen` | `deafen.mp3` | `529ff198eac567af.mp3` |
| `undeafen` | `undeafen.mp3` | `b150f03c89944403.mp3` |
| `stream_start` | `stream_started.mp3` | `abe52a3c92953edb.mp3` |
| `stream_stop` | `stream_ended.mp3` | `cbff07b5a83e0321.mp3` |
| `user_stream` | `stream_user_joined.mp3` | `3d2af2764581bb2e.mp3` |
| `disconnect` | `disconnect.mp3` | `dd0911025fe00b2e.mp3` |
| `message` | `message1.mp3` | `3ed22d14f3c30bc4.mp3` |
| `ring` | `call_ringing.mp3` | `c2a7111bb44b8da0.mp3` |
| `ring_out` | `call_calling.mp3` | `5146737af413d88e.mp3` |
| `ptt_start` | `ptt_start.mp3` | `f5c8ec31889c5317.mp3` |
| `ptt_stop` | `ptt_stop.mp3` | `34b5969be3a063ba.mp3` |
| `camera_on` | `camera_on.mp3` | `855607d0932ea396.mp3` |
| `camera_off` | `camera_off.mp3` | `b1dd204729fe78b4.mp3` |

The `ptt_*` and `camera_*` rows come from the public bundle
`https://discord.com/assets/web.057e7776e1c6251b.js`
(SHA-256 `8694f2d540278e3d14c45eb414e96ada1c64f5a9cbcb52df69347ea35a72574e`),
where the sound maps `./ptt_start.mp3`, `./ptt_stop.mp3`, `./camera_on.mp3` and
`./camera_off.mp3` point at module ids `81025`, `79851`, `423967` and `679283`,
whose exports are those hashed assets. Fetched 2026-10-26
with plain curl.
