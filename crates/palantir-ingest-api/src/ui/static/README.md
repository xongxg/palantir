Place a local copy of D3 here to remove external CDN dependency.

Required file:

  d3.v7.min.js   (MIT License)

How to obtain:
- Option A: Download from https://d3js.org/d3.v7.min.js and save as this filename.
- Option B: `curl -L https://d3js.org/d3.v7.min.js -o d3.v7.min.js`

Runtime path:
- Served at /static/d3.v7.min.js by the ingest API.
- Both /ui (inline viz) and /viz will try /static first, and fallback to the CDN if not found.

Note: If you prefer a fully offline experience, ensure this file exists.
