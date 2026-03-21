#!/usr/bin/env python3
"""Generate the OuroboBackup app icon: blue ouroboros ring with storage safe center."""

import cairo
import math
import os

SIZE = 1024
OUTPUT = os.path.join(os.path.dirname(__file__), "..", "assets", "icon_source.png")


def draw_ouroboros(ctx, cx, cy, radius, num_segments=12):
    """Draw an ouroboros ring made of overlapping teardrop/petal arc segments."""
    blues = [
        (0.10, 0.40, 0.85),  # medium blue
        (0.15, 0.50, 0.95),  # bright blue
        (0.05, 0.30, 0.70),  # dark blue
        (0.20, 0.55, 0.90),  # sky blue
        (0.08, 0.35, 0.80),  # royal blue
        (0.25, 0.60, 0.95),  # light blue
    ]

    segment_angle = 2 * math.pi / num_segments
    petal_length = radius * 0.55
    petal_width = radius * 0.28

    for i in range(num_segments):
        angle = i * segment_angle - math.pi / 2  # start from top
        r, g, b = blues[i % len(blues)]

        ctx.save()
        ctx.translate(cx, cy)
        ctx.rotate(angle)
        ctx.translate(radius, 0)
        ctx.rotate(math.pi / 2 + segment_angle * 0.3)

        # Teardrop/petal shape via two cubic beziers
        ctx.new_path()
        ctx.move_to(0, -petal_length / 2)
        ctx.curve_to(
            petal_width * 0.9, -petal_length * 0.25,
            petal_width * 0.9, petal_length * 0.25,
            0, petal_length / 2,
        )
        ctx.curve_to(
            -petal_width * 0.9, petal_length * 0.25,
            -petal_width * 0.9, -petal_length * 0.25,
            0, -petal_length / 2,
        )
        ctx.close_path()

        ctx.set_source_rgba(r, g, b, 0.88)
        ctx.fill_preserve()
        ctx.set_source_rgba(r * 0.7, g * 0.7, b * 0.7, 0.5)
        ctx.set_line_width(2)
        ctx.stroke()

        ctx.restore()


def draw_safe(ctx, cx, cy, size):
    """Draw a storage safe icon representing secure backup."""
    w = size * 0.38
    h = size * 0.42
    x = cx - w / 2
    y = cy - h / 2
    corner = size * 0.03

    # Shadow
    ctx.save()
    ctx.set_source_rgba(0, 0, 0, 0.15)
    _rounded_rect(ctx, x + 3, y + 4, w, h, corner)
    ctx.fill()
    ctx.restore()

    # Safe body — white with blue border
    ctx.set_source_rgb(1, 1, 1)
    _rounded_rect(ctx, x, y, w, h, corner)
    ctx.fill_preserve()
    ctx.set_source_rgba(0.10, 0.40, 0.85, 0.6)
    ctx.set_line_width(3.5)
    ctx.stroke()

    # Inner border (recessed panel look)
    margin = size * 0.025
    ctx.set_source_rgba(0.10, 0.40, 0.85, 0.15)
    _rounded_rect(ctx, x + margin, y + margin, w - 2 * margin, h - 2 * margin, corner * 0.6)
    ctx.stroke()

    # Dial/combination lock circle in center
    dial_r = size * 0.065
    ctx.set_source_rgba(0.10, 0.40, 0.85, 0.12)
    ctx.arc(cx, cy, dial_r, 0, 2 * math.pi)
    ctx.fill()
    ctx.set_source_rgba(0.10, 0.40, 0.85, 0.5)
    ctx.set_line_width(2.5)
    ctx.arc(cx, cy, dial_r, 0, 2 * math.pi)
    ctx.stroke()

    # Inner dial circle
    inner_r = dial_r * 0.45
    ctx.set_source_rgba(0.10, 0.40, 0.85, 0.6)
    ctx.arc(cx, cy, inner_r, 0, 2 * math.pi)
    ctx.fill()

    # Dial tick marks
    ctx.set_source_rgba(0.10, 0.40, 0.85, 0.5)
    ctx.set_line_width(2)
    for i in range(12):
        angle = i * math.pi / 6
        tick_inner = dial_r * 0.7
        tick_outer = dial_r * 0.95
        ctx.move_to(cx + math.cos(angle) * tick_inner, cy + math.sin(angle) * tick_inner)
        ctx.line_to(cx + math.cos(angle) * tick_outer, cy + math.sin(angle) * tick_outer)
        ctx.stroke()

    # Handle on the right side
    handle_x = x + w - size * 0.045
    handle_y_top = cy - size * 0.055
    handle_y_bot = cy + size * 0.055
    ctx.set_source_rgba(0.10, 0.40, 0.85, 0.55)
    ctx.set_line_width(size * 0.018)
    ctx.set_line_cap(cairo.LINE_CAP_ROUND)
    ctx.move_to(handle_x, handle_y_top)
    ctx.line_to(handle_x, handle_y_bot)
    ctx.stroke()

    # Hinges on the left side
    hinge_x = x + size * 0.015
    for hy_offset in [-h * 0.28, h * 0.28]:
        hy = cy + hy_offset
        ctx.set_source_rgba(0.10, 0.40, 0.85, 0.4)
        _rounded_rect(ctx, hinge_x, hy - size * 0.015, size * 0.02, size * 0.03, size * 0.004)
        ctx.fill()


def _rounded_rect(ctx, x, y, w, h, r):
    """Add a rounded rectangle sub-path."""
    ctx.new_sub_path()
    ctx.arc(x + r, y + r, r, math.pi, 1.5 * math.pi)
    ctx.arc(x + w - r, y + r, r, 1.5 * math.pi, 2 * math.pi)
    ctx.arc(x + w - r, y + h - r, r, 0, 0.5 * math.pi)
    ctx.arc(x + r, y + h - r, r, 0.5 * math.pi, math.pi)
    ctx.close_path()


def main():
    surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, SIZE, SIZE)
    ctx = cairo.Context(surface)

    # White background
    ctx.set_source_rgb(1, 1, 1)
    ctx.paint()

    cx, cy = SIZE / 2, SIZE / 2
    ring_radius = SIZE * 0.32

    draw_ouroboros(ctx, cx, cy, ring_radius, num_segments=12)
    draw_safe(ctx, cx, cy, SIZE * 0.55)

    os.makedirs(os.path.dirname(OUTPUT), exist_ok=True)
    surface.write_to_png(OUTPUT)
    print(f"Generated {OUTPUT} ({SIZE}x{SIZE})")


if __name__ == "__main__":
    main()
