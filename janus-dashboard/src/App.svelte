<script lang="ts">
    import SimpleQuery from "./lib/Query.svelte";
    import StreamChart from "./lib/StreamChart.svelte";

    let isRunning = false;
    let queryId: string | null = null;
    let replayStatus = {
        is_running: false,
        events_read: 0,
        events_published: 0,
    };
    let statusCheckInterval: number | null = null;

    const getDefaultQuery = () => `PREFIX ex: <http://example.org/>
REGISTER RStream ex:output AS
SELECT (AVG(?temp) AS ?avgTemp)
FROM NAMED WINDOW ex:histWindow ON STREAM ex:sensorStream [START 0 END ${Date.now() + 86400000}]
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:sensorStream [RANGE 5000 STEP 2000]
WHERE {
  WINDOW ex:histWindow {
    ?sensor ex:temperature ?temp .
  }
  WINDOW ex:liveWindow {
    ?sensor ex:temperature ?temp .
  }
}`;

    let query = getDefaultQuery();

    function handleQueryChange(newVal: string) {
        query = newVal;
    }

    async function checkReplayStatus() {
        try {
            const response = await fetch(
                "http://localhost:8080/api/replay/status",
            );
            if (response.ok) {
                const status = await response.json();
                replayStatus = status;
                console.log("Replay status:", status);
            }
        } catch (e) {
            console.error("Failed to check replay status:", e);
        }
    }

    async function startReplay() {
        try {
            const response = await fetch(
                "http://localhost:8080/api/replay/start",
                {
                    method: "POST",
                    headers: {
                        "Content-Type": "application/json",
                    },
                    body: JSON.stringify({
                        input_file: "data/realistic_sensors.nq",
                        broker_type: "mqtt",
                        topics: ["sensors"],
                        rate_of_publishing: 64,
                        loop_file: true,
                        add_timestamps: true,
                        mqtt_config: {
                            host: "localhost",
                            port: 1883,
                            client_id: "janus-dashboard",
                            keep_alive_secs: 60,
                        },
                    }),
                },
            );

            if (response.ok) {
                const data = await response.json();
                console.log("Replay has started:", data);
                alert("Replay started successfully!");

                // Start polling status every 2 seconds
                if (statusCheckInterval) {
                    clearInterval(statusCheckInterval);
                }
                statusCheckInterval = window.setInterval(
                    checkReplayStatus,
                    2000,
                );
                checkReplayStatus(); // Check immediately
            } else {
                const errorText = await response.text();
                console.error("Replay error response:", errorText);
                try {
                    const error = JSON.parse(errorText);
                    alert(
                        `Error starting the replay: ${error.error || errorText}`,
                    );
                } catch {
                    alert(`Error starting the replay: ${errorText}`);
                }
            }
        } catch (e) {
            console.error("Failed to start the replay", e);
        }
    }

    async function handleRegisterQuery() {
        console.log("Registering query:", query);
        const qId = "query-" + Date.now(); // Generate a simple ID

        try {
            const response = await fetch("http://localhost:8080/api/queries", {
                method: "POST",
                headers: {
                    "Content-Type": "application/json",
                },
                body: JSON.stringify({
                    query_id: qId,
                    janusql: query,
                }),
            });

            if (response.ok) {
                const data = await response.json();
                console.log("Query registered:", data);
                queryId = data.query_id;
                // Auto-start the query
                await startQuery();
            } else {
                const error = await response.json();
                alert(`Error registering query: ${error.error}`);
            }
        } catch (e) {
            console.error("Failed to register query:", e);
            alert("Failed to connect to backend");
        }
    }

    async function startQuery() {
        if (!queryId) return;

        try {
            const response = await fetch(
                `http://localhost:8080/api/queries/${queryId}/start`,
                {
                    method: "POST",
                },
            );

            if (response.ok) {
                console.log("Query started");
                isRunning = true;
            } else {
                const error = await response.json();
                alert(`Error starting query: ${error.error}`);
            }
        } catch (e) {
            console.error("Failed to start query:", e);
            alert("Failed to connect to backend");
        }
    }

    async function stopQuery() {
        if (!queryId) return;

        try {
            const response = await fetch(
                `http://localhost:8080/api/queries/${queryId}/stop`,
                {
                    method: "POST",
                },
            );

            if (response.ok) {
                console.log("Query stopped");
                isRunning = false;
            } else {
                const error = await response.json();
                alert(`Error stopping query: ${error.error}`);
            }
        } catch (e) {
            console.error("Failed to stop query:", e);
            alert("Failed to connect to backend");
        }
    }
</script>

<main>
    <aside>
        <div class="brand">Janus Dashboard<span class="highlight"></span></div>

        <div class="editor-container">
            <SimpleQuery value={query} onChange={handleQueryChange} />
        </div>

        <div class="controls">
            <div class="status-info">
                <div class="status-line">
                    Replay: {replayStatus.is_running ? "Running" : "Stopped"}
                </div>
                <div class="status-line">
                    Events Read: {replayStatus.events_read}
                </div>
                <div class="status-line">
                    Events Published: {replayStatus.events_published}
                </div>
            </div>

            <button class="register" on:click={startReplay}>
                Start Replay
            </button>

            <button class="register" on:click={handleRegisterQuery}>
                REGISTER QUERY
            </button>


        </div>
    </aside>

    <section>
        <StreamChart {isRunning} {queryId} />
    </section>
</main>

<style>
    :global(body) {
        margin: 0;
        font-family: "Inter", sans-serif;
        background: #f8f9fe;
        color: #333;
        overflow: hidden;
    }

    main {
        display: flex;
        height: 100vh;
        min-width: 1024px; /* Force desktop width */
        overflow-x: auto; /* Allow scrolling if window is smaller */
    }

    aside {
        width: 350px;
        background: #ffffff;
        border-right: 1px solid #e0e0e0;
        padding: 20px;
        display: flex;
        flex-direction: column;
        gap: 20px;
        box-shadow: 2px 0 5px rgba(0, 0, 0, 0.02);
        padding-bottom: 40px; /* Prevent buttons from being cut off */
        overflow-y: auto; /* Allow scrolling if content is too tall */
    }

    section {
        flex: 1;
        position: relative;
        background: #f8f9fe;
        display: flex;
        flex-direction: column;
        padding: 20px;
        min-width: 600px; /* Ensure space for chart */
    }

    .brand {
        font-size: 1.5rem;
        font-weight: 800;
        color: #6200ea; /* Deep Purple */
        margin-bottom: 10px;
    }
    .highlight {
        color: #2196f3; /* Blue */
    }

    /* Stream Descriptor Cards - REMOVED */

    .editor-container {
        flex: 1;
    }

    .controls {
        display: flex;
        flex-direction: column;
        gap: 12px;
    }

    button {
        width: 100%;
        padding: 16px;
        background: #6200ea; /* Deep Purple */
        color: white;
        font-weight: 600;
        border: none;
        border-radius: 12px;
        cursor: pointer;
        font-size: 1rem;
        transition: all 0.2s;
        box-shadow: 0 4px 12px rgba(98, 0, 234, 0.2);
    }

    button.register {
        background: #ffffff;
        color: #6200ea;
        border: 2px solid #6200ea;
        box-shadow: none;
    }
    button.register:hover {
        background: #f3e5f5;
        transform: translateY(-1px);
    }

    button:hover {
        background: #7c4dff;
        transform: translateY(-1px);
        box-shadow: 0 6px 16px rgba(98, 0, 234, 0.3);
    }
    button.stop {
        background: #ff1744;
        color: white;
        box-shadow: 0 4px 12px rgba(255, 23, 68, 0.2);
    }
    button.stop:hover {
        background: #d50000;
    }

    .status-info {
        background: #f8f9fe;
        border: 1px solid #e0e0e0;
        border-radius: 8px;
        padding: 12px;
        margin-bottom: 12px;
        font-size: 0.85rem;
    }

    .status-line {
        margin: 4px 0;
        color: #666;
        font-weight: 500;
    }
</style>
