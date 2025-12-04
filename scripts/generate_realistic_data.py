import random
import math

# Generate 1000 data points
num_points = 1000
base_temp = 23.0

with open("data/realistic_sensors.nq", "w") as f:
    for i in range(num_points):
        # Use a dummy timestamp (will be replaced by server during replay)
        timestamp = 1000 * i

        # Create a sine wave + random noise pattern
        # Sine wave period: 100 points
        sine_component = 2.0 * math.sin(i * 2 * math.pi / 100)

        # Random noise: +/- 0.5
        noise = random.uniform(-0.5, 0.5)

        # Sensor 1: Base + Sine + Noise
        val1 = base_temp + sine_component + noise
        f.write(f"{timestamp} <http://example.org/sensor1> <http://example.org/temperature> \"{val1:.2f}\"^^<http://www.w3.org/2001/XMLSchema#decimal> <http://example.org/sensorStream> .\n")

        # Sensor 2: Base + Cosine + Noise (slightly different phase)
        cosine_component = 2.0 * math.cos(i * 2 * math.pi / 100)
        noise2 = random.uniform(-0.5, 0.5)
        val2 = base_temp + cosine_component + noise2
        f.write(f"{timestamp} <http://example.org/sensor2> <http://example.org/temperature> \"{val2:.2f}\"^^<http://www.w3.org/2001/XMLSchema#decimal> <http://example.org/sensorStream> .\n")

print(f"Generated data/realistic_sensors.nq with {num_points} points")
