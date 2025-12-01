<script lang="ts">
    import * as echarts from "echarts";
    import { onDestroy, onMount } from "svelte";

    export let isRunning: boolean = false;
    export let queryId: string | null = null;

    let chartContainer: HTMLDivElement;
    let chartInstance: echarts.ECharts;
    let ws: WebSocket | null = null;

    // Mutable buffer in the scope of the script.
    // Separate buffers for live and historical data
    const liveBuffer: { time: number; value: number }[] = [];
    const histBuffer: { time: number; value: number }[] = [];

    onMount(() => {
        chartInstance = echarts.init(chartContainer);

        // Initial Chart Configuration
        chartInstance.setOption({
            backgroundColor: "transparent",
            animation: false,
            tooltip: {
                trigger: "axis",
                backgroundColor: "rgba(255, 255, 255, 0.95)",
                borderColor: "#eee",
                textStyle: {
                    color: "#333",
                },
                extraCssText:
                    "box-shadow: 0 2px 8px rgba(0,0,0,0.1); border-radius: 8px;",
            },
            legend: {
                show: true,
                top: "10px",
                right: "20px",
                textStyle: {
                    color: "#333",
                    fontWeight: "bold",
                },
                itemGap: 20,
            },
            grid: {
                left: "20px",
                right: "80px", /* Increased right to make room for markLine label */
                bottom: "20px",
                top: "60px",
                containLabel: true,
            },
            xAxis: {
                type: "time",
                splitLine: { show: false },
                axisLabel: {
                    color: "#666",
                    fontWeight: "bold",
                    formatter: function (value: number) {
                        const date = new Date(value);
                        const year = date.getFullYear();
                        const month = (date.getMonth() + 1).toString().padStart(2, "0");
                        const day = date.getDate().toString().padStart(2, "0");
                        const hours = date
                            .getHours()
                            .toString()
                            .padStart(2, "0");
                        const minutes = date
                            .getMinutes()
                            .toString()
                            .padStart(2, "0");
                        const seconds = date
                            .getSeconds()
                            .toString()
                            .padStart(2, "0");
                        return `${year}-${month}-${day}\n${hours}:${minutes}:${seconds}`;
                    },
                },
                axisLine: { lineStyle: { color: "#ccc" } },
            },
            yAxis: {
                type: "value",
                splitLine: { lineStyle: { color: "#eee" } },
                axisLabel: { color: "#666" },
            },
            series: [
                {
                    name: "Live Data",
                    type: "line",
                    smooth: true,
                    symbolSize: 8,
                    itemStyle: {
                        color: "#2196f3", // Blue
                    },
                    lineStyle: {
                        width: 3,
                    },
                    data: [],
                },
                {
                    name: "Historical Data",
                    type: "line",
                    smooth: true,
                    symbol: "none", // Show line only in legend
                    itemStyle: {
                        color: "#ff9800", // Orange
                    },
                    lineStyle: {
                        width: 3,
                        type: "dashed",
                    },
                    data: [],
                },
            ],
        });

        const handleResize = () => chartInstance.resize();
        window.addEventListener("resize", handleResize);

        // Force a resize after a short delay to ensure container is ready
        setTimeout(() => {
            chartInstance.resize();
        }, 100);

        return () => {
            window.removeEventListener("resize", handleResize);
            chartInstance.dispose();
        };
    });

    $: if (isRunning && queryId) {
        connectBackend();
    } else {
        disconnectBackend();
    }

    function connectBackend() {
        if (!queryId) return;

        const wsUrl = `ws://localhost:8080/api/queries/${queryId}/results`;
        console.log("Connecting to WebSocket:", wsUrl);

        try {
            ws = new WebSocket(wsUrl);

            ws.onopen = () => {
                console.log("WebSocket connected");
            };

            ws.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    const timestamp = data.timestamp; // Assuming timestamp is in ms or convert if needed
                    const source = data.source;

                    // bindings can be an array of results
                    const bindingsArray = Array.isArray(data.bindings)
                        ? data.bindings
                        : [data.bindings];

                    bindingsArray.forEach((binding: Record<string, string>) => {
                        // Backend returns keys without '?' prefix
                        // Try common variable names used in queries
                        const valStr =
                            binding["avgTemp"] ||
                            binding["temp"] ||
                            binding["val"] ||
                            binding["tempHist"] ||
                            binding["?avgTemp"] ||
                            binding["?temp"] ||
                            binding["?val"] ||
                            binding["?tempHist"];

                        if (valStr) {
                            // Handle typed literals like "23.5"^^<...> or "23.5"
                            let cleanValStr = valStr;
                            if (valStr.includes("^^")) {
                                cleanValStr = valStr.split("^^")[0];
                            }
                            // Remove quotes
                            cleanValStr = cleanValStr.replace(/['"]+/g, "");

                            const val = Number(cleanValStr);

                            if (!isNaN(val)) {
                                const cleanSource = source ? source.trim().toLowerCase() : "";
                                const point = { time: timestamp, value: val };

                                if (cleanSource === "historical") {
                                    histBuffer.push(point);
                                } else {
                                    // Filter out invalid points (0 timestamp or 0 value which are likely artifacts)
                                    if (point.time > 0 && point.value > 0) {
                                        liveBuffer.push(point);
                                    }
                                }
                            } else {
                                console.warn(
                                    "Parsed value is NaN for:",
                                    valStr,
                                );
                            }
                        }
                    });

                    // Keep live buffer size manageable
                    if (liveBuffer.length > 1000) {
                        const removeCount = liveBuffer.length - 1000;
                        liveBuffer.splice(0, removeCount);
                    }

                    updateChart();
                } catch (e) {
                    console.error("Error parsing WebSocket message:", e);
                }
            };

            ws.onerror = (error) => {
                console.error("WebSocket error:", error);
            };

            ws.onclose = () => {
                console.log("WebSocket disconnected");
            };
        } catch (error) {
            console.error("Failed to connect to backend:", error);
        }
    }

    function disconnectBackend() {
        if (ws) {
            ws.close();
            ws = null;
        }
    }

    function updateChart() {
        if (!chartInstance) return;

        const liveData = liveBuffer.map((d) => [d.time, d.value]);

        // For historical data:
        // If we have a single point (aggregate), we use it for markLine BUT NOT for the series data
        // to avoid stretching the X-axis to the future timestamp.
        // If we have multiple points (e.g. sliding window), we plot them.
        let histData: number[][] = [];
        let markLine = {};

        if (histBuffer.length === 1) {
            const val = histBuffer[0].value;
            // Do NOT add to histData to avoid axis scaling issues with future timestamp
            histData = [];

            markLine = {
                data: [
                    {
                        yAxis: val,
                        label: {
                            formatter: `Avg: ${val.toFixed(2)}`,
                            position: "end",
                        },
                    },
                ],
                lineStyle: {
                    color: "#ff9800",
                    type: "dashed",
                    width: 2,
                },
                symbol: "none",
            };
        } else {
            histData = histBuffer.map((d) => [d.time, d.value]);
        }

        chartInstance.setOption({
            series: [
                {
                    name: "Live Data",
                    data: liveData,
                },
                {
                    name: "Historical Data",
                    data: histData,
                    markLine: markLine,
                },
            ],
        });
    }

    onDestroy(() => {
        disconnectBackend();
    });
</script>

<div class="chart-wrapper" bind:this={chartContainer}></div>

<style>
    .chart-wrapper {
        flex: 1;
        width: 100%;
        /* height: 600px;  Removed fixed height */
        aspect-ratio: 16 / 9; /* Enforce 16:9 aspect ratio */
        min-height: 400px;
        max-height: 85vh; /* Prevent it from being too tall on large screens */
        min-width: 100%; /* Allow full expansion */
        border-radius: 12px;
        background: #ffffff;
        border: 1px solid #eee;
        box-shadow: 0 4px 20px rgba(0, 0, 0, 0.02);
    }
</style>
