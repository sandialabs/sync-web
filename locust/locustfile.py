import os
import json
import random
from locust import HttpUser, task


class HelloWorldUser(HttpUser):
    @task
    def hello_world(self):
        key = f"key-{random.randint(0, 2**20)}"
        val = f"val-{random.randint(0, 2**20)}"

        request_data = {
            "function": "set!",
            "arguments": {
                "path": [["*state*", "locust", key]],
                "value": {"*type/string*": val},
            },
            "authentication": {"*type/string*": os.environ["SECRET"]},
        }
        response = self.client.post("/interface/json", json=request_data)

        # Truncate request and response for readable logging
        request_text = json.dumps(request_data, separators=(",", ":"))
        req_truncated = (
            request_text[:80] + "..." if len(request_text) > 80 else request_text
        )
        resp_truncated = (
            response.text[:80] + "..." if len(response.text) > 80 else response.text
        )

        print(f"REQ: {req_truncated} | RESP: {resp_truncated}")
