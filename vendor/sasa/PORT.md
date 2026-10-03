Based on https://github.com/Mivik/sasa at e76229b2f68e4dc68b22bc375d2a92e8e7897691.

Local extension: music-only noise-field low pass, controlled through an atomic
flag (no new commands in the bounded playback queue). The audio thread handles
the 1500 Hz / 22000 Hz, 100 ms cutoff sweep at its actual output sample rate.
Pause and seek reset the filter; unrelated music and SFX remain bypassed.
The existing set_low_pass API is preserved.

The resonant biquad is an adaptation, not extracted Unity DSP code. No Phigros
binary, song, or other asset is included here. See ../../docs/block-area-port.md.
