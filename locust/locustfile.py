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
            "path": [["*state*", "locust", key]],
            "value": {"*type/string*": val},
        }
        response = self.client.post(
            "/api/v1/general/set",
            json=request_data,
            headers={"Authorization": f"Bearer {os.environ['SECRET']}"},
        )

        # Truncate request and response for readable logging
        request_text = json.dumps(request_data, separators=(",", ":"))
        req_truncated = (
            request_text[:80] + "..." if len(request_text) > 80 else request_text
        )
        resp_truncated = (
            response.text[:80] + "..." if len(response.text) > 80 else response.text
        )

        print(f"REQ: {req_truncated} | RESP: {resp_truncated}")
