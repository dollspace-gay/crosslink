#!/usr/bin/env python3
"""
Generate a Forecast-styled diagram for multi-agent coordination.

Usage:
    python3 scripts/generate-diagram-multi-agent.py -o docs_src/assets/img/multi-agent-flow.svg
"""

import argparse
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from brand import (P, svg_header, svg_footer, ellipse, circle, rrect, text,
                   arrow_straight, confetti, pill, container, write_svg)

WIDTH = 680
HEIGHT = 520
SEED = 19


def agent_node(x, y, name, color, issue):
    """Draw an agent as a colored ellipse with identity and locked issue."""
    svg = ellipse(x, y, 75, 45, color, opacity=0.12)
    svg += ellipse(x, y, 65, 37, P["white"], opacity=0.85)
    svg += text(x, y - 8, name, cls="mono", size=12, fill=color, weight="bold")
    svg += text(x, y + 12, f"working #{issue}", cls="body", size=11, fill=P["muted"])
    return svg


def generate():
    rng = random.Random(SEED)
    svg = svg_header(WIDTH, HEIGHT)

    cx = WIDTH / 2

    # ── Title ─────────────────────────────────────────────────────────────
    svg += text(cx, 36, "Multi-agent coordination", cls="heading", size=22, fill=P["black"])
    svg += text(cx, 56, "distributed locking via crosslink/hub",
                cls="subheading", size=14, fill=P["muted"])

    # ── Three agent nodes (top row) ───────────────────────────────────────
    agents = [
        (130,  130, "agent-frontend", P["blue"],  12),
        (340,  130, "agent-backend",  P["green"], 15),
        (550,  130, "agent-infra",    P["red"],   18),
    ]
    for ax, ay, name, color, issue in agents:
        svg += agent_node(ax, ay, name, color, issue)

    # ── Coordination branch (center hub) ──────────────────────────────────
    hub_y = 270
    svg += container(80, hub_y - 40, 520, 100, P["yellow"], "crosslink/hub branch")

    # Lock pills inside the hub
    for lx, lw, color, label in [
        (110, 150, P["blue"],  "#12 → frontend"),
        (280, 150, P["green"], "#15 → backend"),
        (450, 150, P["red"],   "#18 → infra"),
    ]:
        svg += pill(lx, hub_y, lw, 28, color, label, rx=14)

    # ── Arrows: agents → hub ──────────────────────────────────────────────
    for ax, color in [(130, P["blue"]), (340, P["green"]), (550, P["red"])]:
        target_x = min(max(ax, 150), 530)
        svg += arrow_straight(ax, 175, target_x, hub_y - 45,
                              color, stroke_width=1.5, dashed=True)

    # ── Sync label ────────────────────────────────────────────────────────
    svg += text(cx, hub_y + 78, "sync via git push/pull to coordination branch",
                cls="body", size=12, fill=P["muted"])

    # ── Daemon indicator ──────────────────────────────────────────────────
    svg += rrect(470, hub_y + 62, 155, 28, P["green"], rx=14, opacity=0.12)
    svg += circle(484, hub_y + 76, 4, P["green"])
    svg += text(548, hub_y + 80, "daemon running", cls="body", size=11, fill=P["green"])

    # ── Bottom: result summary ────────────────────────────────────────────
    svg += rrect(60, 410, WIDTH - 120, 80, P["gray"], rx=20)
    svg += text(cx, 438, "Agents self-coordinate — no manual lock management",
                cls="heading", size=15, fill=P["black"])

    features = ["lock before work", "heartbeat monitoring",
                "stale lock detection", "signature verification"]
    for i, label in enumerate(features):
        fx = 100 + i * 140
        svg += circle(fx, 462, 4, P["green"])
        svg += text(fx + 12, 466, label, cls="body", size=11, fill=P["text"], anchor="start")

    # ── Confetti ──────────────────────────────────────────────────────────
    svg += confetti(rng, 10, 80, 50, 80, 5)
    svg += confetti(rng, 620, 80, 50, 80, 5)

    svg += svg_footer()
    return svg


def main():
    parser = argparse.ArgumentParser(description="Generate multi-agent coordination diagram SVG")
    parser.add_argument("-o", "--output", help="Output file (default: stdout)")
    args = parser.parse_args()
    write_svg(generate(), args)


if __name__ == "__main__":
    main()
