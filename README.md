<div align="center">

<img src="./assets/full_logo_dark.png#gh-dark-mode-only" alt="HelixDB Logo">
<img src="./assets/full_logo_light.png#gh-light-mode-only" alt="HelixDB Logo">

<b>HelixDB</b>: a graph-vector database for knowledge graphs and AI memory. Built from scratch in Rust.
<br/><br/>
<a href="https://www.ycombinator.com/launches/Naz-helixdb-the-database-for-rag-ai" target="_blank"><img src="https://www.ycombinator.com/launches/Naz-helixdb-the-database-for-rag-ai/upvote_embed.svg" alt="Launch YC: HelixDB - The Database for Intelligence" style="margin-left: 12px;"/></a>
<h3>
  <a href="https://helix-db.com">website</a> |
  <a href="https://docs.helix-db.com">docs</a> |
  <a href="https://discord.gg/2stgMPr5BD">discord</a> |
  <a href="https://x.com/helixdb">X/twitter</a>
</h3>

[![Docs](https://img.shields.io/badge/docs-latest-blue)](https://docs.helix-db.com)
[![Change Log](https://img.shields.io/badge/changelog-latest-blue)](https://docs.helix-db.com/change-log/helixdb)
[![GitHub Repo stars](https://img.shields.io/github/stars/HelixDB/helix-db)](https://github.com/HelixDB/helix-db/stargazers)
[![Discord](https://img.shields.io/discord/1354148209005559819?logo=discord)](https://discord.gg/2stgMPr5BD)
[![LOC](https://img.shields.io/endpoint?url=https://ghloc.vercel.app/api/HelixDB/helix-db/badge?filter=.rs$,.sh$&style=flat&logoColor=white&label=Lines%20of%20Code)](https://github.com/HelixDB/helix-db)



</div>

<hr>


HelixDB is a database that makes it easy to build all the components needed for AI applications in a single platform.

You don't need a separate application DB, relational DB, vector DB, graph DB, or application layers to manage the multiple storage locations. HelixDB gives your agents federated access to company data, for memory, company brains, and applications.

Helix primarily operates with a graph + vector data model, but it also supports KV, documents, and relational data.

### Get started with HelixDB

## Getting Started

Start by installing the Helix CLI tool to deploy Helix locally.

1. Install CLI

   ```bash
   curl -sSL "https://install.helix-db.com" | bash
   ```

2. Initialize a project

   ```bash
   mkdir <path-to-project> && cd <path-to-project>
   helix init
   ```

3. Start a local development instance

   ```bash
   helix run dev
   ```

4. Send a dynamic query

   `helix init` creates `examples/request.json`, which is a ready-to-run dynamic query request.

   ```bash
   helix query dev --file examples/request.json
   ```

   Dynamic query requests are JSON payloads sent to `POST /v1/query`:

   ```json
   {
     "request_type": "read",
     "query": {
       "queries": [{
         "Query": {
           "name": "node_count",
           "steps": [
             {"NWhere": {"Eq": ["$label", {"String": "User"}]}},
             "Count"
           ],
           "condition": null
         }
       }],
       "returns": ["node_count"]
     },
     "parameters": {}
   }
   ```

5. Stop the local instance when finished

   ```bash
    helix stop dev
    ```

### Enterprise Cloud Deployments

Enterprise Cloud clusters use a separate deploy path. After linking an Enterprise instance with `helix init enterprise` or `helix add enterprise`.


## Commercial Support

### GA Cloud
HelixDB is available as a managed service, if you're interested in using Helix's managed service, go to [our website](https://helix-db.com/login) to get started.

### Enterprise
HelixDB is available as a distributed, high-availability cluster for customers in need of INFINTE scale. If you're interested in enterprise support, [contact us](mailto:founders@helix-db.com).

---

Just Use Helix.
