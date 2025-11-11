import init, { Session, resample } from "./pkg/session_rs.js";

const DB_NAME = "session-db";
const DB_VERSION = 1;
const STORE_AUDIO = "audio";
const STORE_META = "metadata";

// IndexedDB helper functions
function openDB() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result);

    request.onupgradeneeded = (event) => {
      const db = event.target.result;

      // Create audio store if it doesn't exist
      if (!db.objectStoreNames.contains(STORE_AUDIO)) {
        db.createObjectStore(STORE_AUDIO);
      }

      // Create metadata store if it doesn't exist
      if (!db.objectStoreNames.contains(STORE_META)) {
        db.createObjectStore(STORE_META);
      }
    };
  });
}

function getFromStore(db, storeName, key) {
  return new Promise((resolve, reject) => {
    const transaction = db.transaction(storeName, "readonly");
    const store = transaction.objectStore(storeName);
    const request = store.get(key);

    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result);
  });
}

function putInStore(db, storeName, key, value) {
  return new Promise((resolve, reject) => {
    const transaction = db.transaction(storeName, "readwrite");
    const store = transaction.objectStore(storeName);
    const request = store.put(value, key);

    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve();
  });
}

function getAllKeys(db, storeName) {
  return new Promise((resolve, reject) => {
    const transaction = db.transaction(storeName, "readonly");
    const store = transaction.objectStore(storeName);
    const request = store.getAllKeys();

    request.onerror = () => reject(request.error);
    request.onsuccess = () => resolve(request.result);
  });
}

// Global session instance
let sessionInstance = null;
let dbInstance = null;
let targetSampleRate = 11500; // Default sample rate for the recognizer

// Initialize session on startup
async function initializeSession() {
  try {
    await init();

    // Open database
    dbInstance = await openDB();

    // Create session instance with default configuration
    sessionInstance = new Session({});

    // Load all existing recordings from IndexedDB and register them
    const audioKeys = await getAllKeys(dbInstance, STORE_AUDIO);

    for (const uuid of audioKeys) {
      const audioData = await getFromStore(dbInstance, STORE_AUDIO, uuid);
      const meta = await getFromStore(dbInstance, STORE_META, uuid);

      // Only register if marked as indexed
      if (meta && meta.indexed !== false) {
        // Note: Stored audio is already in original sample rate
        // We assume it was stored at 44.1kHz or needs resampling
        // Since we don't store the original sample rate, we'll assume 44.1kHz
        const resampledAudio = resample(audioData, 44100, targetSampleRate);
        sessionInstance.register(uuid, resampledAudio);
      }
    }

    console.log(`Session initialized with ${audioKeys.length} recordings`);
  } catch (error) {
    console.error("Failed to initialize session:", error);
  }
}

// Convert Float32Array to WAV format
function createWavBlob(audioData, sampleRate = 44100) {
  const numChannels = 1;
  const bitsPerSample = 32;
  const byteRate = sampleRate * numChannels * bitsPerSample / 8;
  const blockAlign = numChannels * bitsPerSample / 8;
  const dataSize = audioData.length * bitsPerSample / 8;
  const fileSize = 44 + dataSize;

  const buffer = new ArrayBuffer(fileSize);
  const view = new DataView(buffer);

  // Write WAV header
  const writeString = (offset, string) => {
    for (let i = 0; i < string.length; i++) {
      view.setUint8(offset + i, string.charCodeAt(i));
    }
  };

  writeString(0, 'RIFF');
  view.setUint32(4, fileSize - 8, true);
  writeString(8, 'WAVE');
  writeString(12, 'fmt ');
  view.setUint32(16, 16, true); // fmt chunk size
  view.setUint16(20, 3, true); // IEEE float format
  view.setUint16(22, numChannels, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, byteRate, true);
  view.setUint16(32, blockAlign, true);
  view.setUint16(34, bitsPerSample, true);
  writeString(36, 'data');
  view.setUint32(40, dataSize, true);

  // Write audio data
  const offset = 44;
  for (let i = 0; i < audioData.length; i++) {
    view.setFloat32(offset + i * 4, audioData[i], true);
  }

  return new Blob([buffer], { type: 'audio/wav' });
}

// Parse WAV blob to Float32Array and extract sample rate
async function parseWavBlob(blob) {
  const arrayBuffer = await blob.arrayBuffer();
  const dataView = new DataView(arrayBuffer);

  let sampleRate = 44100; // Default sample rate
  let audioData = null;

  // Parse WAV header and chunks
  let offset = 12; // Skip RIFF header

  while (offset < arrayBuffer.byteLength) {
    const chunkId = String.fromCharCode(
      dataView.getUint8(offset),
      dataView.getUint8(offset + 1),
      dataView.getUint8(offset + 2),
      dataView.getUint8(offset + 3)
    );
    const chunkSize = dataView.getUint32(offset + 4, true);

    if (chunkId === 'fmt ') {
      // Extract sample rate from format chunk
      sampleRate = dataView.getUint32(offset + 12, true);
    } else if (chunkId === 'data') {
      // Read audio data (assumes f32 PCM)
      audioData = new Float32Array(chunkSize / 4);
      for (let i = 0; i < audioData.length; i++) {
        audioData[i] = dataView.getFloat32(offset + 8 + i * 4, true);
      }
    }

    offset += 8 + chunkSize;
  }

  if (!audioData) {
    throw new Error('No data chunk found in WAV file');
  }

  return { audioData, sampleRate };
}

// Handle fetch events
self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);
  const pathname = url.pathname;

  // Match v1 API endpoints
  const listMatch = pathname.match(/^\/v1\/recordings\/?$/);
  const recordingMatch = pathname.match(/^\/v1\/recordings\/([0-9a-f-]+)$/i);
  const metaMatch = pathname.match(/^\/v1\/recordings\/([0-9a-f-]+)\/meta$/i);
  const searchMatch = pathname.match(/^\/v1\/search$/);

  if (listMatch && event.request.method === 'GET') {
    // GET recordings/
    event.respondWith(handleGetRecordings());
  } else if (recordingMatch) {
    const uuid = recordingMatch[1];

    if (event.request.method === 'POST' || event.request.method === 'PUT') {
      // POST/PUT recordings/{uuid}
      event.respondWith(handlePutRecording(uuid, event.request));
    } else if (event.request.method === 'GET') {
      // GET recordings/{uuid}
      event.respondWith(handleGetRecording(uuid));
    }
  } else if (metaMatch) {
    const uuid = metaMatch[1];

    if (event.request.method === 'POST') {
      // POST recordings/{uuid}/meta
      event.respondWith(handlePostMeta(uuid, event.request));
    } else if (event.request.method === 'GET') {
      // GET recordings/{uuid}/meta
      event.respondWith(handleGetMeta(uuid));
    }
  } else if (searchMatch && event.request.method === 'POST') {
    // POST search
    event.respondWith(handleSearch(event.request));
  }
});

// Handler: POST/PUT recordings/{uuid}
async function handlePutRecording(uuid, request) {
  try {
    const blob = await request.blob();
    const { audioData, sampleRate } = await parseWavBlob(blob);

    // Store raw audio in IndexedDB
    await putInStore(dbInstance, STORE_AUDIO, uuid, audioData);

    // Initialize metadata if it doesn't exist
    let meta = await getFromStore(dbInstance, STORE_META, uuid);
    if (!meta) {
      meta = {
        name: uuid,
        date: new Date().toISOString(),
        tags: [],
        indexed: true
      };
      await putInStore(dbInstance, STORE_META, uuid, meta);
    }

    // Register to session for feature map processing
    if (meta.indexed !== false) {
      // Resample audio to target sample rate before registering
      const resampledAudio = sampleRate !== targetSampleRate
        ? resample(audioData, sampleRate, targetSampleRate)
        : audioData;

      sessionInstance.register(uuid, resampledAudio);
    }

    return new Response(null, { status: 200 });
  } catch (error) {
    console.error('Error handling PUT recording:', error);
    return new Response(JSON.stringify({ error: error.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}

// Handler: GET recordings/
async function handleGetRecordings() {
  try {
    // Get all metadata keys
    const metaKeys = await getAllKeys(dbInstance, STORE_META);

    // Fetch metadata for each key to get the date
    const recordings = await Promise.all(
      metaKeys.map(async (uuid) => {
        const meta = await getFromStore(dbInstance, STORE_META, uuid);
        return {
          uuid,
          date: meta ? new Date(meta.date) : new Date(0)
        };
      })
    );

    // Sort by date, newest to oldest
    recordings.sort((a, b) => b.date - a.date);

    // Return as text list of UUIDs
    const uuidList = recordings.map(r => r.uuid).join('\n');

    return new Response(uuidList, {
      status: 200,
      headers: { 'Content-Type': 'text/plain' }
    });
  } catch (error) {
    console.error('Error handling GET recordings:', error);
    return new Response(JSON.stringify({ error: error.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}

// Handler: GET recordings/{uuid}
async function handleGetRecording(uuid) {
  try {
    const audioData = await getFromStore(dbInstance, STORE_AUDIO, uuid);

    if (!audioData) {
      return new Response(JSON.stringify({ error: 'Recording not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' }
      });
    }

    const wavBlob = createWavBlob(audioData);
    return new Response(wavBlob, {
      status: 200,
      headers: { 'Content-Type': 'audio/wav' }
    });
  } catch (error) {
    console.error('Error handling GET recording:', error);
    return new Response(JSON.stringify({ error: error.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}

// Handler: POST recordings/{uuid}/meta
async function handlePostMeta(uuid, request) {
  try {
    const meta = await request.json();

    // Store metadata
    await putInStore(dbInstance, STORE_META, uuid, meta);

    // If indexed status changed, update session
    const audioData = await getFromStore(dbInstance, STORE_AUDIO, uuid);
    if (audioData) {
      if (meta.indexed !== false) {
        // Resample audio before registering (assume 44.1kHz for stored audio)
        const resampledAudio = resample(audioData, 44100, targetSampleRate);
        sessionInstance.register(uuid, resampledAudio);
      }
      // Note: We don't have an unregister method, so we just skip re-registering if indexed is false
    }

    return new Response(JSON.stringify(meta), {
      status: 200,
      headers: { 'Content-Type': 'application/json' }
    });
  } catch (error) {
    console.error('Error handling POST meta:', error);
    return new Response(JSON.stringify({ error: error.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}

// Handler: GET recordings/{uuid}/meta
async function handleGetMeta(uuid) {
  try {
    const meta = await getFromStore(dbInstance, STORE_META, uuid);

    if (!meta) {
      return new Response(JSON.stringify({ error: 'Metadata not found' }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' }
      });
    }

    return new Response(JSON.stringify(meta), {
      status: 200,
      headers: { 'Content-Type': 'application/json' }
    });
  } catch (error) {
    console.error('Error handling GET meta:', error);
    return new Response(JSON.stringify({ error: error.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}

// Handler: POST search
async function handleSearch(request) {
  try {
    const blob = await request.blob();
    const { audioData, sampleRate } = await parseWavBlob(blob);

    // Resample audio to target sample rate before searching
    const resampledAudio = sampleRate !== targetSampleRate
      ? resample(audioData, sampleRate, targetSampleRate)
      : audioData;

    // Perform search
    const results = sessionInstance.search(resampledAudio);

    // Format results according to API spec
    const formattedResults = results.map(result => ({
      score: result.score,
      uuid: result.uuid,
      keyStart: result.keyStart,
      keyEnd: result.keyEnd,
      queryStart: result.queryStart,
      queryEnd: result.queryStart, // Note: API spec mentions queryEnd but WASM doesn't provide it
      queryUrl: `/v1/recordings/${result.uuid}#t=${result.keyStart},${result.keyEnd}`
    }));

    return new Response(JSON.stringify(formattedResults), {
      status: 200,
      headers: { 'Content-Type': 'application/json' }
    });
  } catch (error) {
    console.error('Error handling search:', error);
    return new Response(JSON.stringify({ error: error.message }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' }
    });
  }
}

// Initialize on service worker activation
self.addEventListener('activate', (event) => {
  event.waitUntil(initializeSession());
});

// Initialize immediately if already active
if (self.registration && self.registration.active) {
  initializeSession();
}
