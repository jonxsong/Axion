"""Semantic search example; requires `axion-client[semantic-search]`."""

from sentence_transformers import SentenceTransformer

from axion_client import AxionClient

documents = [
    "Rust gives systems programmers memory safety without garbage collection.",
    "HNSW navigates a layered proximity graph for approximate search.",
    "RocksDB provides an embedded log-structured storage engine.",
]
model = SentenceTransformer("sentence-transformers/all-MiniLM-L6-v2")
embeddings = model.encode(documents, normalize_embeddings=True).tolist()

with AxionClient("http://localhost:8080") as client:
    client.create_collection("semantic-demo", dimensions=len(embeddings[0]), metric="cosine")
    client.upsert(
        "semantic-demo",
        [
            {"id": f"doc-{index}", "values": embedding, "metadata": {"text": text}}
            for index, (text, embedding) in enumerate(zip(documents, embeddings, strict=True))
        ],
    )
    query = model.encode("How does approximate vector search work?", normalize_embeddings=True)
    for result in client.search("semantic-demo", query.tolist(), top_k=2).results:
        print(f"{result.score:.4f}  {result.metadata['text']}")

