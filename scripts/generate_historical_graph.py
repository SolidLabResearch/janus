import time

# Current time in milliseconds
now = int(time.time() * 1000)

# 1 hour ago
one_hour_ago = now - (60 * 60 * 1000)

# Generate 100 data points starting from 1 hour ago, spaced by 1 second
with open("data/sensors_historical_graph.nq", "w") as f:
    for i in range(100):
        timestamp = one_hour_ago + (i * 1000)

        # Sensor 1
        f.write(f"{timestamp} <http://example.org/sensor1> <http://example.org/temperature> \"{20 + (i % 5)}\"^^<http://www.w3.org/2001/XMLSchema#decimal> <http://example.org/sensorStream> .\n")

        # Sensor 2
        f.write(f"{timestamp} <http://example.org/sensor2> <http://example.org/temperature> \"{22 + (i % 5)}\"^^<http://www.w3.org/2001/XMLSchema#decimal> <http://example.org/sensorStream> .\n")

print(f"Generated data/sensors_historical_graph.nq with start timestamp {one_hour_ago}")
