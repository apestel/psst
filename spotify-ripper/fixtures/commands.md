curl -i -X POST http://localhost:3000/api/download_track_from_spotify -H 'Content-Type: application/json' -d @test_pubsub_push.json

curl -X POST http://localhost:3000/api/request_tracks_download -H 'Content-Type: application/json' -d @tracks.json

curl -X POST http://localhost:3000/api/search_album -H 'Content-Type: application/json' -d '{"album_name": "Abba"}' | jq '.[].id'

curl -X POST http://localhost:3000/api/get_tracks_status -H 'Content-Type: application/json' -d @tracks.json
