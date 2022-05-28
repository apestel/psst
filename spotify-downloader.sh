#!/bin/sh
for x in *.ogg; do ffmpeg -i "$x" -ab 320k "`basename "$x" .ogg`.m4a"; done
