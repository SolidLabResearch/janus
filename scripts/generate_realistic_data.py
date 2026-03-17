#!/usr/bin/env python3
import argparse
import random
import math
import os


def main():
    parser = argparse.ArgumentParser(description='Generate realistic sensor data as N-Quads')
    parser.add_argument('--size', type=int, default=1000, help='Number of events to generate')
    parser.add_argument('--output', type=str, default='data/realistic_sensors.nq', help='Output file path')
    args = parser.parse_args()

    num_points = args.size
    output_path = args.output
    base_temp = 23.0
    base_ts = 1_000_000  # Start timestamp in milliseconds, 1s intervals

    out_dir = os.path.dirname(output_path)
    if out_dir:
        os.makedirs(out_dir, exist_ok=True)

    with open(output_path, "w") as f:
        for i in range(num_points):
            timestamp = base_ts + i * 1000  # 1-second intervals

            # Sine wave + noise for sensor 1
            sine_component = 2.0 * math.sin(i * 2 * math.pi / 100)
            noise = random.uniform(-0.5, 0.5)
            val1 = base_temp + sine_component + noise
            f.write(
                f"<http://example.org/sensor1> "
                f"<http://example.org/temperature> "
                f"\"{val1:.2f}\"^^<http://www.w3.org/2001/XMLSchema#decimal> "
                f"<http://example.org/sensorStream> . #{timestamp}\n"
            )

            # Cosine wave + noise for sensor 2
            cosine_component = 2.0 * math.cos(i * 2 * math.pi / 100)
            noise2 = random.uniform(-0.5, 0.5)
            val2 = base_temp + cosine_component + noise2
            f.write(
                f"<http://example.org/sensor2> "
                f"<http://example.org/temperature> "
                f"\"{val2:.2f}\"^^<http://www.w3.org/2001/XMLSchema#decimal> "
                f"<http://example.org/sensorStream> . #{timestamp}\n"
            )

    print(f"Generated {output_path} with {num_points} points")


if __name__ == '__main__':
    main()
