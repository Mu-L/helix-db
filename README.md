<div align="center">

<picture>
  <img src="/assets/full_logo.png" alt="HelixDB Logo">
</picture>

<b>HelixDB</b>: an open-source graph-vector database built from scratch in Rust.

<h3>
  <a href="https://helix-db.com">Website</a> |
  <a href="https://docs.helix-db.com">Docs</a> |
  <a href="https://discord.gg/2stgMPr5BD">Discord</a> |
  <a href="https://x.com/helixdb">X/Twitter</a>
</h3>

[![Docs](https://img.shields.io/badge/docs-latest-blue)](https://docs.helix-db.com)
[![Change Log](https://img.shields.io/badge/changelog-latest-blue)](https://docs.helix-db.com/change-log/helixdb)
[![GitHub Repo stars](https://img.shields.io/github/stars/HelixDB/helix-db)](https://github.com/HelixDB/helix-db/stargazers)
[![Discord](https://img.shields.io/discord/1354148209005559819?logo=discord)](https://discord.gg/2stgMPr5BD)
[![LOC](https://img.shields.io/endpoint?url=https://ghloc.vercel.app/api/HelixDB/helix-db/badge?filter=.rs$,.sh$&style=flat&logoColor=white&label=Lines%20of%20Code)](https://github.com/HelixDB/helix-db)
[![Manta Graph](https://getmanta.ai/api/badges?text=Manta%20Graph&link=helixdb)](https://getmanta.ai/helixdb)

<a href="https://www.ycombinator.com/launches/Naz-helixdb-the-database-for-rag-ai" target="_blank"><img src="https://www.ycombinator.com/launches/Naz-helixdb-the-database-for-rag-ai/upvote_embed.svg" alt="Launch YC: HelixDB - The Database for Intelligence" style="margin-left: 12px;"/></a>

</div>

<hr>


HelixDB is a database that makes it easy to build all the components needed for an AI application in a single platform.

You no longer need a separate application DB, vector DB, graph DB, or application layers to manage the multiple storage locations to build the backend of any application that uses AI, agents or RAG. Just use Helix.

HelixDB primarily operates with a graph + vector data model, but it can also support KV, documents, and relational data.

### Get started with HelixDB

<div align="center">                                                                                                                                                                                                                                                                                                                                                                                   
    <img src="/assets/readmeinit.gif" alt="Helix CLI Demo" width="100%">                                                                                                                                                                                                                                                                                                                                              
</div>  

--- 

## Key Features

|                         |                                                                                                                                        |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| **Built-in MCP tools**  | Helix has built-in MCP support to allow your agents to discover data and walk the graph rather than generating human readable queries. |
| **Built-in Embeddings** | No need to embed your data before sending it to Helix, just use the `Embed` function to vectorize text.                                |
| **Tooling for RAG**     | HelixDB has a built-in vector search, keyword search, and graph traversals that can be used to power any type of RAG applications.     |
| **Secure by Default**   | HelixDB is private by default and can be run locally or through Enterprise Cloud.                                                     |
| **Ultra-Low Latency**   | Helix is built in Rust and uses LMDB as its storage engine to provide extremely low latencies.                                         |
| **Dynamic Queries**     | The v2 query API accepts JSON requests through `POST /v1/query`, so local development does not require a compile/deploy loop.          |

## Getting Started

#### Helix CLI

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

3. Start a local v2 development instance

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

Enterprise Cloud clusters use a separate deploy path. After linking an Enterprise instance with `helix init enterprise` or `helix add enterprise`, run `helix push <instance>` to compile and upload the Enterprise query project, and `helix sync <instance>` to reconcile source snapshots and refresh cloud metadata.

## License

HelixDB is licensed under the The AGPL (Affero General Public License).

## Commercial Support

HelixDB is available as a managed service for selected users, if you're interested in using Helix's managed service or want enterprise support, [contact](mailto:founders@helix-db.com) us for more information and deployment options.

---

Just Use Helix
