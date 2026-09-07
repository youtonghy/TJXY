`xing.mp3` is a generated 0.2-second 1 kHz sine wave (no third-party content):

```sh
ffmpeg -v error -f lavfi -i sine=frequency=1000:duration=0.2 -c:a libmp3lame -q:a 2 -map_metadata -1 xing.mp3
```

The Xing header has flags `0x0000000f` and nine frames at 44.1 kHz.
The probe reports encoded frame duration, including encoder delay and padding.
The checked-in fixture keeps tests independent of an installed encoder.
