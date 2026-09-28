#!/usr/bin/env python3
"""Record docs/examples/pydantic-ai-shop.otlp.jsonl: an agent's OpenTelemetry
spans, as OTLP JSON, for `stretto learn --otel`.

The agent is a Pydantic AI agent with the demo shop's four tools
(`stretto-mcp-demo --world retail`'s). For each of eight customers it finds
them by email, reads their details, reads both of their orders in one turn,
and cancels the order they asked it to. Its model is scripted (Pydantic
AI's `FunctionModel`), so no LLM is called and every run writes the same
spans but for their ids and times. The spans are Pydantic AI's own
instrumentation, which follows OpenTelemetry's GenAI semantic conventions,
with content capture on: the conventions' opt-in tool arguments and
results, and the messages. Each line is one trace's export, as the
OpenTelemetry Collector's file exporter writes a batch.

    python3 -m venv /tmp/otel && /tmp/otel/bin/pip install \\
        pydantic-ai-slim==2.51.0 opentelemetry-sdk==1.45.0 \\
        opentelemetry-exporter-otlp-proto-common==1.45.0
    /tmp/otel/bin/python scripts/otel_fixture.py docs/examples/pydantic-ai-shop.otlp.jsonl
"""

import argparse
import base64
import json
import re

from google.protobuf.json_format import MessageToDict
from opentelemetry.exporter.otlp.proto.common.trace_encoder import encode_spans
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from opentelemetry.sdk.trace.export.in_memory_span_exporter import InMemorySpanExporter
from pydantic_ai import Agent
from pydantic_ai.capabilities.instrumentation import Instrumentation
from pydantic_ai.messages import (
    ModelMessage,
    ModelRequest,
    ModelResponse,
    TextPart,
    ToolCallPart,
    ToolReturnPart,
    UserPromptPart,
)
from pydantic_ai.models.function import AgentInfo, FunctionModel
from pydantic_ai.models.instrumented import InstrumentationSettings

CUSTOMERS = 8


def orders(n: str) -> list[str]:
    return [f"#W{n}a", f"#W{n}b"]


def model(messages: list[ModelMessage], info: AgentInfo) -> ModelResponse:
    """The scripted agent: each step once, in order."""
    prompt = next(
        p.content
        for m in messages
        if isinstance(m, ModelRequest)
        for p in m.parts
        if isinstance(p, UserPromptPart)
    )
    n = re.search(r"c(\d+)@", prompt).group(1)
    returned = {
        p.tool_name: p.content
        for m in messages
        if isinstance(m, ModelRequest)
        for p in m.parts
        if isinstance(p, ToolReturnPart)
    }
    call = ToolCallPart
    if "find_user_id_by_email" not in returned:
        return ModelResponse(parts=[call("find_user_id_by_email", {"email": f"c{n}@example.com"})])
    if "get_user_details" not in returned:
        user = returned["find_user_id_by_email"]
        return ModelResponse(parts=[call("get_user_details", {"user_id": user})])
    if "get_order_details" not in returned:
        # Both orders at once: one turn, two calls.
        found = returned["get_user_details"]["orders"]
        return ModelResponse(parts=[call("get_order_details", {"order_id": o}) for o in found])
    if "cancel_pending_order" not in returned:
        return ModelResponse(
            parts=[
                TextPart("Both orders are pending; cancelling the one you named."),
                call("cancel_pending_order", {"order_id": orders(n)[0], "reason": "no longer needed"}),
            ]
        )
    return ModelResponse(parts=[TextPart(f"Done: order {orders(n)[0]} is cancelled.")])


def shop(provider: TracerProvider) -> Agent:
    agent = Agent(
        FunctionModel(model, model_name="scripted-shop-model"),
        name="shop",
        instructions="You help the shop's customers with their orders.",
        capabilities=[
            Instrumentation(
                settings=InstrumentationSettings(
                    tracer_provider=provider,
                    include_content=True,
                    # Pydantic AI's own copy of each request's tool schemas.
                    include_model_request_parameters=False,
                )
            )
        ],
    )

    @agent.tool_plain
    def find_user_id_by_email(email: str) -> str:
        """Find a customer's user id by their email."""
        return "user_" + email.split("@")[0][1:]

    @agent.tool_plain
    def get_user_details(user_id: str) -> dict:
        """A customer's details, with their orders."""
        n = user_id.split("_")[1]
        return {"user_id": user_id, "email": f"c{n}@example.com", "orders": orders(n)}

    @agent.tool_plain
    def get_order_details(order_id: str) -> dict:
        """An order's status and items."""
        return {"order_id": order_id, "status": "pending", "items": [{"item_id": "1", "name": "Desk lamp"}]}

    @agent.tool_plain
    def cancel_pending_order(order_id: str, reason: str) -> dict:
        """Cancel a pending order."""
        return {"order_id": order_id, "status": "cancelled", "reason": reason}

    return agent


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("out", help="where to write the OTLP JSON")
    args = parser.parse_args()

    exporter = InMemorySpanExporter()
    provider = TracerProvider(resource=Resource.create({"service.name": "shop-agent"}))
    provider.add_span_processor(SimpleSpanProcessor(exporter))
    agent = shop(provider)
    for n in range(CUSTOMERS):
        agent.run_sync(f"Hi, I'm c{n}@example.com. Please cancel order #W{n}a, I no longer need it.")

    traces: dict[int, list] = {}
    for span in exporter.get_finished_spans():
        traces.setdefault(span.context.trace_id, []).append(span)
    with open(args.out, "w") as f:
        for spans in traces.values():
            export = MessageToDict(encode_spans(spans), use_integers_for_enums=True)
            # OTLP JSON writes trace and span ids in hex, where protobuf's JSON uses base64.
            for resource in export["resourceSpans"]:
                for scope in resource["scopeSpans"]:
                    for span in scope["spans"]:
                        for key in ("traceId", "spanId", "parentSpanId"):
                            if key in span:
                                span[key] = base64.b64decode(span[key]).hex()
            f.write(json.dumps(export, separators=(",", ":")) + "\n")
    spans = sum(len(s) for s in traces.values())
    print(f"wrote {spans} spans of {len(traces)} traces to {args.out}")


if __name__ == "__main__":
    main()
