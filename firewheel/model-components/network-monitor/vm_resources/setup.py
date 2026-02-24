#!/usr/bin/env python3

import os
import json
import time

from base64 import b64encode
from urllib.request import Request, urlopen
from urllib.error import URLError


def fetch(resource, data=None):
    auth_string = b64encode(
        f"{os.environ['GRAFANA_USER']}:{os.environ['GRAFANA_PASSWORD']}".encode()
    ).decode()

    request = Request(
        f"http://localhost:3000/{resource}",
        method="POST" if data else "GET",
        headers={
            "Authorization": f"Basic {auth_string}",
            "Content-Type": "application/json",
        },
        data=json.dumps(data).encode() if data else None,
    )

    return json.loads(urlopen(request).read().decode())


while True:
    try:
        result = fetch("api/admin/stats")
        assert result["users"] >= 1
        print("Connected to Grafana server")
        break
    except (ConnectionResetError, URLError) as e:
        print("Failed to connect to Grafana server (retrying in 1s)", e)
        time.sleep(1)


url = os.environ["PROMETHEUS"]

result = fetch(                 # 
    "api/datasources",
    {
        "name": "journals",
        "type": "prometheus",
        "url": os.environ["PROMETHEUS"],
        "access": "proxy",
    },
)

uid = result["datasource"]["uid"]

print(f"Created datasource with uid: {uid}")

result = fetch(
    "apis/dashboard.grafana.app/v1beta1/namespaces/default/dashboards",
    {
        "metadata": {"name": "journal-dashboard"},
        "spec": {
            "editable": True,
            "links": [],
            "panels": [
                {
                    "datasource": {"type": "prometheus", "uid": uid},
                    "description": "",
                    "fieldConfig": {
                        "defaults": {
                            "color": {"mode": "palette-classic"},
                            "custom": {
                                "axisBorderShow": False,
                                "axisCenteredZero": False,
                                "axisColorMode": "text",
                                "axisLabel": "Ongoing CPU Usage",
                                "axisPlacement": "auto",
                                "barAlignment": 0,
                                "barWidthFactor": 0.6,
                                "drawStyle": "line",
                                "fillOpacity": 0,
                                "gradientMode": "none",
                                "hideFrom": {
                                    "legend": False,
                                    "tooltip": False,
                                    "viz": False,
                                },
                                "insertNulls": False,
                                "lineInterpolation": "linear",
                                "lineWidth": 1,
                                "pointSize": 5,
                                "scaleDistribution": {"type": "linear"},
                                "showPoints": "auto",
                                "showValues": False,
                                "spanNulls": False,
                                "stacking": {"group": "A", "mode": "none"},
                                "thresholdsStyle": {"mode": "off"},
                            },
                            "mappings": [],
                            "thresholds": {
                                "mode": "absolute",
                                "steps": [
                                    {"color": "green", "value": 0},
                                    {"color": "red", "value": 80},
                                ],
                            },
                            "unit": "percentunit",
                        },
                        "overrides": [],
                    },
                    "gridPos": {"h": 8, "w": 12, "x": 0, "y": 0},
                    "id": 1,
                    "options": {
                        "legend": {
                            "calcs": [],
                            "displayMode": "list",
                            "placement": "bottom",
                            "showLegend": True,
                        },
                        "tooltip": {
                            "hideZeros": False,
                            "mode": "single",
                            "sort": "none",
                        },
                    },
                    "pluginVersion": "12.2.0-17567790421",
                    "targets": [
                        {
                            "datasource": {"type": "prometheus", "uid": uid},
                            "editorMode": "builder",
                            "expr": 'sum by(instance) (rate(node_cpu_seconds_total{job="journal-monitor",mode!="idle"}[1m]))',
                            "hide": False,
                            "instant": False,
                            "legendFormat": "__auto",
                            "range": True,
                            "refId": "A",
                        }
                    ],
                    "title": "CPU",
                    "type": "timeseries",
                },
                {
                    "datasource": {"type": "prometheus", "uid": uid},
                    "description": "",
                    "fieldConfig": {
                        "defaults": {
                            "color": {"mode": "palette-classic"},
                            "custom": {
                                "axisBorderShow": False,
                                "axisCenteredZero": False,
                                "axisColorMode": "text",
                                "axisLabel": "Total Memory Usage",
                                "axisPlacement": "auto",
                                "barAlignment": 0,
                                "barWidthFactor": 0.6,
                                "drawStyle": "line",
                                "fillOpacity": 0,
                                "gradientMode": "none",
                                "hideFrom": {
                                    "legend": False,
                                    "tooltip": False,
                                    "viz": False,
                                },
                                "insertNulls": False,
                                "lineInterpolation": "linear",
                                "lineWidth": 1,
                                "pointSize": 5,
                                "scaleDistribution": {"type": "linear"},
                                "showPoints": "auto",
                                "showValues": False,
                                "spanNulls": False,
                                "stacking": {"group": "A", "mode": "none"},
                                "thresholdsStyle": {"mode": "off"},
                            },
                            "mappings": [],
                            "thresholds": {
                                "mode": "absolute",
                                "steps": [
                                    {"color": "green", "value": 0},

                                    {"color": "red", "value": 80},
                                ],
                            },
                            "unit": "bytes",
                        },
                        "overrides": [],
                    },
                    "gridPos": {"h": 8, "w": 12, "x": 12, "y": 0},
                    "id": 2,
                    "options": {
                        "legend": {
                            "calcs": [],
                            "displayMode": "list",
                            "placement": "bottom",
                            "showLegend": True,
                        },
                        "tooltip": {
                            "hideZeros": False,
                            "mode": "single",
                            "sort": "none",
                        },
                    },
                    "pluginVersion": "12.2.0-17567790421",
                    "targets": [
                        {
                            "editorMode": "builder",
                            "expr": 'sum by(instance) (node_memory_MemAvailable_bytes{job="journal-monitor"})',
                            "hide": True,
                            "legendFormat": "__auto",
                            "range": True,
                            "refId": "A",
                        },
                        {
                            "datasource": {"type": "prometheus", "uid": uid},
                            "editorMode": "builder",
                            "expr": 'sum by(instance) (node_memory_MemTotal_bytes{job="journal-monitor"})',
                            "hide": True,
                            "instant": False,
                            "legendFormat": "__auto",
                            "range": True,
                            "refId": "B",
                        },
                        {
                            "datasource": {
                                "name": "Expression",
                                "type": "__expr__",
                                "uid": "__expr__",
                            },
                            "expression": "($B - $A)",
                            "hide": False,
                            "refId": "*",
                            "type": "math",
                        },
                    ],
                    "title": "Memory",
                    "type": "timeseries",
                },
                {
                    "datasource": {"type": "prometheus", "uid": uid},
                    "description": "",
                    "fieldConfig": {
                        "defaults": {
                            "color": {"mode": "palette-classic"},
                            "custom": {
                                "axisBorderShow": False,
                                "axisCenteredZero": False,
                                "axisColorMode": "text",
                                "axisLabel": "Total Storage Usage",
                                "axisPlacement": "auto",
                                "barAlignment": 0,
                                "barWidthFactor": 0.6,
                                "drawStyle": "line",
                                "fillOpacity": 0,
                                "gradientMode": "none",
                                "hideFrom": {
                                    "legend": False,
                                    "tooltip": False,
                                    "viz": False,
                                },
                                "insertNulls": False,
                                "lineInterpolation": "linear",
                                "lineWidth": 1,
                                "pointSize": 5,
                                "scaleDistribution": {"type": "linear"},
                                "showPoints": "auto",
                                "showValues": False,
                                "spanNulls": False,
                                "stacking": {"group": "A", "mode": "none"},
                                "thresholdsStyle": {"mode": "off"},
                            },
                            "mappings": [],
                            "thresholds": {
                                "mode": "absolute",
                                "steps": [
                                    {"color": "green", "value": 0},
                                    {"color": "red", "value": 80},
                                    {"color": "#EAB839", "value": 90},
                                ],
                            },
                            "unit": "bytes",
                        },
                        "overrides": [],
                    },
                    "gridPos": {"h": 8, "w": 12, "x": 0, "y": 8},
                    "id": 3,
                    "options": {
                        "legend": {
                            "calcs": [],
                            "displayMode": "list",
                            "placement": "bottom",
                            "showLegend": True,
                        },
                        "tooltip": {
                            "hideZeros": False,
                            "mode": "single",
                            "sort": "none",
                        },
                    },
                    "pluginVersion": "12.2.0-17567790421",
                    "targets": [
                        {
                            "editorMode": "builder",
                            "expr": 'sum by(instance) (node_filesystem_avail_bytes{job="journal-monitor"})',
                            "hide": True,
                            "legendFormat": "__auto",
                            "range": True,
                            "refId": "A",
                        },
                        {
                            "datasource": {"type": "prometheus", "uid": uid},
                            "editorMode": "builder",
                            "expr": 'sum by(instance) (node_filesystem_size_bytes{job="journal-monitor"})',
                            "hide": True,
                            "instant": False,
                            "legendFormat": "__auto",
                            "range": True,
                            "refId": "B",
                        },
                        {
                            "datasource": {
                                "name": "Expression",
                                "type": "__expr__",
                                "uid": "__expr__",
                            },
                            "expression": "$B - $A",
                            "hide": False,
                            "refId": "*",
                            "type": "math",
                        },
                    ],
                    "title": "Storage",
                    "type": "timeseries",
                },
                {
                    "datasource": {"type": "prometheus", "uid": uid},
                    "description": "",
                    "fieldConfig": {
                        "defaults": {
                            "color": {"mode": "palette-classic"},
                            "custom": {
                                "axisBorderShow": False,
                                "axisCenteredZero": False,
                                "axisColorMode": "text",
                                "axisLabel": "Ongoing Network Usage",
                                "axisPlacement": "auto",
                                "barAlignment": 0,
                                "barWidthFactor": 0.6,
                                "drawStyle": "line",
                                "fillOpacity": 0,
                                "gradientMode": "none",
                                "hideFrom": {
                                    "legend": False,
                                    "tooltip": False,
                                    "viz": False,
                                },
                                "insertNulls": False,
                                "lineInterpolation": "linear",
                                "lineWidth": 1,
                                "pointSize": 5,
                                "scaleDistribution": {"type": "linear"},
                                "showPoints": "auto",
                                "showValues": False,
                                "spanNulls": False,
                                "stacking": {"group": "A", "mode": "none"},
                                "thresholdsStyle": {"mode": "off"},
                            },
                            "mappings": [],
                            "thresholds": {
                                "mode": "absolute",
                                "steps": [
                                    {"color": "green", "value": 0},
                                    {"color": "red", "value": 80},
                                ],
                            },
                            "unit": "binBps",
                        },
                        "overrides": [],
                    },
                    "gridPos": {"h": 8, "w": 12, "x": 12, "y": 8},
                    "id": 4,
                    "options": {
                        "legend": {
                            "calcs": [],
                            "displayMode": "list",
                            "placement": "bottom",
                            "showLegend": True,
                        },
                        "tooltip": {
                            "hideZeros": False,
                            "mode": "single",
                            "sort": "none",
                        },
                    },
                    "pluginVersion": "12.2.0-17567790421",
                    "targets": [
                        {
                            "editorMode": "builder",
                            "exemplar": False,
                            "expr": 'sum by(instance) (rate(node_network_receive_bytes_total{job="journal-monitor"}[1m]))',
                            "hide": True,
                            "instant": False,
                            "legendFormat": "{{label_name}}",
                            "range": True,
                            "refId": "A",
                        },
                        {
                            "datasource": {
                                "name": "Expression",
                                "type": "__expr__",
                                "uid": "__expr__",
                            },
                            "expression": "$A",
                            "hide": False,
                            "refId": "B",
                            "type": "math",
                        },
                    ],
                    "title": "Networking",
                    "type": "timeseries",
                },
            ],
            "preload": False,
            "refresh": "10s",
            "schemaVersion": 42,
            "tags": [],
            "templating": {"list": []},
            "time": {"from": "now-1h", "to": "now"},
            "timepicker": {},
            "timezone": "browser",
            "title": "Journal Dashboard",
        },
        "status": {},
    },
)

uid = result["metadata"]["uid"]

print(f"Created dashboard with uid: {uid}")

url = os.environ["PROMETHEUS"]

result = fetch(                 # 
    "api/datasources",
    {
        "name": "agents",
        "type": "prometheus",
        "url": os.environ["PROMETHEUS"],
        "access": "proxy",
    },
)

uid = result["datasource"]["uid"]

print(f"Created datasource with uid: {uid}")

window_variable = {
    "name": "window",
    "label": "Window",
    "type": "custom",
    "hide": 0,
    "includeAll": False,
    "multi": False,
    "query": "30s,1m,5m,15m",
    "options": [
        {"text": "30s", "value": "30s", "selected": False},
        {"text": "1m", "value": "1m", "selected": True},
        {"text": "5m", "value": "5m", "selected": False},
        {"text": "15m", "value": "15m", "selected": False},
    ],
    "current": {"text": "1m", "value": "1m", "selected": True},
}

result = fetch(
    "apis/dashboard.grafana.app/v1beta1/namespaces/default/dashboards",
    {
        "metadata": {"name": "agent-dashboard"},
        "spec": {
            "editable": True,
            "links": [],
            "panels": [
                {
                    "id": 1,
                    "title": "Request Throughput",
                    "type": "timeseries",
                    "datasource": {"type": "prometheus", "uid": uid},
                    "gridPos": {"h": 8, "w": 12, "x": 0, "y": 0},
                    "fieldConfig": {"defaults": {"unit": "ops"}, "overrides": []},
                    "pluginVersion": "12.2.0-17567790421",
                    "targets": [
                        {
                            "refId": "A",
                            "datasource": {"type": "prometheus", "uid": uid},
                            "expr": 'sum by(instance) (rate(social_agent_activity_cycles_success_total{job="agent-monitor"}[$window]))',
                            "editorMode": "code",
                            "legendFormat": "{{instance}}",
                            "range": True,
                            "hide": False,
                        }
                    ],
                },
                {
                    "id": 2,
                    "title": "Success Rate",
                    "type": "timeseries",
                    "datasource": {"type": "prometheus", "uid": uid},
                    "gridPos": {"h": 8, "w": 12, "x": 12, "y": 0},
                    "fieldConfig": {"defaults": {"unit": "percentunit"}, "overrides": []},
                    "pluginVersion": "12.2.0-17567790421",
                    "targets": [
                        {
                            "refId": "A",
                            "datasource": {"type": "prometheus", "uid": uid},
                            "expr": 'sum by(instance) (rate(social_agent_activity_cycles_success_total{job="agent-monitor"}[$window])) / clamp_min(sum by(instance) (rate(social_agent_activity_cycles_total{job="agent-monitor"}[$window])), 1e-9)',
                            "editorMode": "code",
                            "legendFormat": "{{instance}}",
                            "range": True,
                            "hide": False,
                        }
                    ],
                },
                {
                    "id": 3,
                    "title": "Get Latency",
                    "type": "timeseries",
                    "datasource": {"type": "prometheus", "uid": uid},
                    "gridPos": {"h": 8, "w": 12, "x": 0, "y": 8},
                    "fieldConfig": {"defaults": {"unit": "s"}, "overrides": []},
                    "pluginVersion": "12.2.0-17567790421",
                    "targets": [
                        {
                            "refId": "A",
                            "datasource": {"type": "prometheus", "uid": uid},
                            "expr": 'sum by(instance) (rate(social_agent_get_latency_seconds_sum{job="agent-monitor"}[$window])) / clamp_min(sum by(instance) (rate(social_agent_get_latency_seconds_count{job="agent-monitor"}[$window])), 1e-9)',
                            "editorMode": "code",
                            "legendFormat": "{{instance}}",
                            "range": True,
                            "hide": False,
                        }
                    ],
                },
                {
                    "id": 4,
                    "title": "Set Latency",
                    "type": "timeseries",
                    "datasource": {"type": "prometheus", "uid": uid},
                    "gridPos": {"h": 8, "w": 12, "x": 12, "y": 8},
                    "fieldConfig": {"defaults": {"unit": "s"}, "overrides": []},
                    "pluginVersion": "12.2.0-17567790421",
                    "targets": [
                        {
                            "refId": "A",
                            "datasource": {"type": "prometheus", "uid": uid},
                            "expr": 'sum by(instance) (rate(social_agent_set_latency_seconds_sum{job="agent-monitor"}[$window])) / clamp_min(sum by(instance) (rate(social_agent_set_latency_seconds_count{job="agent-monitor"}[$window])), 1e-9)',
                            "editorMode": "code",
                            "legendFormat": "{{instance}}",
                            "range": True,
                            "hide": False,
                        }
                    ],
                },
            ],
            "preload": False,
            "refresh": "10s",
            "schemaVersion": 42,
            "tags": [],
            "templating": {"list": [window_variable]},
            "time": {"from": "now-1h", "to": "now"},
            "timepicker": {},
            "timezone": "browser",
            "title": "Agent Dashboard",
        },
        "status": {},
    },
)

agent_uid = result["metadata"]["uid"]
print(f"Created dashboard with uid: {agent_uid}")
