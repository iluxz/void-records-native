// Regenerates assets/tracks.json + assets/updates.json from the web app source.
// Run: node sync-assets.js
const fs = require('fs');
const path = require('path');
const web = path.resolve(__dirname, '..', 'void-records');
const out = path.resolve(__dirname, 'assets');

const trackSrc = fs.readFileSync(path.join(web, 'tracks.js'), 'utf8');
const tracks = JSON.parse(trackSrc.match(/const TRACKS=(\[[\s\S]*\])$/)[1]);

// updates.js mixes the PATCH_LOG literal with DOM wiring, so isolate just the array.
const updateSrc = fs.readFileSync(path.join(web, 'updates.js'), 'utf8');
const start = updateSrc.indexOf('[');
const end = updateSrc.indexOf('\n];', start);
if (start < 0 || end < 0) throw new Error('could not find PATCH_LOG array in updates.js');
const logs = new Function('return ' + updateSrc.slice(start, end + 2))();

fs.writeFileSync(path.join(out, 'tracks.json'), JSON.stringify(tracks, null, 2) + '\n');
fs.writeFileSync(path.join(out, 'updates.json'), JSON.stringify(logs, null, 2) + '\n');
console.log('tracks', tracks.length, 'logs', logs.length);
