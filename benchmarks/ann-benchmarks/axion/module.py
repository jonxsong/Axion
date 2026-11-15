import http.client
import json
import os
import shutil
import signal
import subprocess
import tempfile
import time

import psutil

from ann_benchmarks.algorithms.base.module import BaseANN


class Axion(BaseANN):
    """Official ANN-Benchmarks adapter for the Axion REST service."""

    def __init__(self, metric, method_param):
        metrics = {"angular": "cosine", "euclidean": "euclidean", "dot": "dot"}
        if metric not in metrics:
            raise ValueError("Axion does not support ANN-Benchmarks metric %r" % metric)
        self.metric = metrics[metric]
        self.method_param = method_param
        self.ef_search = method_param.get("efSearch", 64)
        self.port = 8080
        self.connection = None
        self.process = None
        self.data_dir = None
        self.name = "Axion (%s, efSearch=%d)" % (method_param, self.ef_search)

    def _connect(self):
        if self.connection is not None:
            self.connection.close()
        self.connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=120)

    def _request(self, method, path, body=None):
        if self.connection is None:
            self._connect()
        encoded = None if body is None else json.dumps(body, separators=(",", ":"))
        headers = {"Content-Type": "application/json"} if encoded is not None else {}
        try:
            self.connection.request(method, path, body=encoded, headers=headers)
            response = self.connection.getresponse()
            payload = response.read()
        except (BrokenPipeError, ConnectionError, http.client.HTTPException):
            self._connect()
            self.connection.request(method, path, body=encoded, headers=headers)
            response = self.connection.getresponse()
            payload = response.read()
        if response.status < 200 or response.status >= 300:
            raise RuntimeError(
                "Axion returned HTTP %d for %s %s: %s"
                % (response.status, method, path, payload.decode("utf-8", "replace"))
            )
        return json.loads(payload) if payload else None

    def _start_server(self):
        self.data_dir = tempfile.mkdtemp(prefix="axion-ann-benchmarks-")
        environment = os.environ.copy()
        environment.update(
            {
                "AXION_DATA_DIR": self.data_dir,
                "AXION_HOST": "127.0.0.1",
                "AXION_HTTP_PORT": str(self.port),
                "AXION_GRPC_PORT": "50051",
                "RUST_LOG": "error",
            }
        )
        self.process = subprocess.Popen(
            ["/usr/local/bin/axion-server"],
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError("Axion exited before its readiness probe passed")
            try:
                self._connect()
                if self._request("GET", "/health/ready")["status"] == "ready":
                    return
            except (ConnectionError, OSError, http.client.HTTPException):
                pass
            time.sleep(0.1)
        raise RuntimeError("Axion did not become ready within 60 seconds")

    def fit(self, X):
        self._start_server()
        dimensions = int(len(X[0]))
        self._request(
            "POST",
            "/v1/collections",
            {
                "name": "ann_benchmarks",
                "dimensions": dimensions,
                "metric": self.metric,
                "hnsw": {
                    "m": int(self.method_param["M"]),
                    "ef_construction": int(self.method_param["efConstruction"]),
                    "ef_search": int(self.ef_search),
                    "random_seed": 2802896897,
                },
            },
        )

        # Keep the encoded request comfortably below Axion's 8 MiB body limit.
        batch_size = max(1, min(1000, 2_000_000 // (dimensions * 12)))
        for start in range(0, len(X), batch_size):
            vectors = [
                {"id": str(index), "values": X[index].tolist()}
                for index in range(start, min(start + batch_size, len(X)))
            ]
            self._request(
                "POST",
                "/v1/collections/ann_benchmarks/vectors/batch",
                {"vectors": vectors},
            )

    def set_query_arguments(self, ef_search):
        self.ef_search = int(ef_search)
        self.name = "Axion (%s, efSearch=%d)" % (self.method_param, self.ef_search)

    def query(self, vector, n):
        response = self._request(
            "POST",
            "/v1/collections/ann_benchmarks/search",
            {
                "vector": vector.tolist(),
                "top_k": int(n),
                "mode": "approximate",
                "ef_search": self.ef_search,
            },
        )
        return [int(result["id"]) for result in response["results"]]

    def get_memory_usage(self):
        if self.process is None or self.process.poll() is not None:
            return 0.0
        return psutil.Process(self.process.pid).memory_info().rss / 1024.0

    def done(self):
        if self.connection is not None:
            self.connection.close()
            self.connection = None
        if self.process is not None and self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        self.process = None
        if self.data_dir is not None:
            shutil.rmtree(self.data_dir, ignore_errors=True)
            self.data_dir = None

