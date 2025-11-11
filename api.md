# Session Service Worker API Endpoints

On sessions service worker startup, read all the songs from the indexdb and register them to the session index. 

## `v1` endpoint

This is for an MVP, so don't worry about any async for now...

- POST/PUT `recordings/{uuid}`
    - Accepts blob of f32 mono 44.1kHz WAV audio
    - Stores raw audio in IndexedDB
    - Begins feature map processing
    - Returns 200 Accepted upon body stored and feature map generation
- GET `recordings/{uuid}`
    - Returns body with f32 mono 44.1kHz wav audio
    - Might be worth to support partial?
- GET `recordings/`
    - Returns text list of all available uuids, sorted from latest to oldest by date.
- POST/GET `recordings/{uuid}/meta`
    - Gets and sets JSON metadata for the given audio track
        - `name`: user-supplied name
        - `date`: creation timestamp
        - `tags`: user-generated tags
        - `indexed`: whether or not the recording is indexed and searchable
- POST `search`
    - Accepts stream of f32 mono 44.1kHz PCM audio.
    - Upon body completion, return JSON array of matches:
        - `score`: similarity score. lower is better.
        - `uuid`: key UUID
        - `keyStart`, `keyEnd`: timestamps of matching segment in key
        - `queryStart`, `queryEnd`: timestamps of matching segment in query
        - `queryUrl`: convenience URL of query segment, using URL hash format: `recordings/{uuid}#t={keyStart},{keyEnd}`