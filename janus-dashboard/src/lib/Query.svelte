<script lang="ts">
    export let value: string;
    export let onChange: (val: string) => void;

    function handleInput(e: Event) {
        const target = e.target as HTMLAreaElement;
        onChange(target.value);
    }
</script>

<div class="query-box">
    <label for="">Janus-QL Query Input</label>
    <textarea spellcheck="false" {value} on:input={handleInput}></textarea>
</div>

<style>
    .query-box {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: #ffffff;
        border: 1px solid #eee;
        border-radius: 12px;
        padding: 16px;
        box-shadow: 0 2px 8px rgba(0,0,0,0.04);
    }

    label {
        font-size: 0.75rem;
        color: #666;
        font-weight: 700;
        margin-bottom: 8px;
        letter-spacing: 0.5px;
        text-transform: uppercase;
    }

    textarea {
        flex: 1;
        background: #f8f9fe;
        color: #333;
        font-family: "Consolas", "Monaco", monospace;
        font-size: 0.9rem;
        border: 1px solid #e0e0e0;
        border-radius: 8px;
        resize: none;
        outline: none;
        line-height: 1.5;
        padding: 12px;
        transition: border-color 0.2s, background 0.2s;
    }

    textarea:focus {
        background: #ffffff;
        border-color: #2196f3;
    }
</style>
