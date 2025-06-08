# Runs a wash command in all workspace members in crates/nodes/*
wash-run-all command="build":
    #!/usr/bin/env bash
    for dir in crates/nodes/*/; do
        if [ -d "$dir" ]; then
            echo "👷 Running command '{{command}}' in $dir ..."
            cd "$dir"
            wash {{command}}
            cd - > /dev/null
        fi
    done

# Pushes all workspace members in crates/nodes/* to the registry
wash-push-all: (wash-run-all "build")
    #!/usr/bin/env bash
    for dir in crates/nodes/*/; do
        if [ -d "$dir" ]; then
            crate_name=$(basename "$dir")
            if [ "$crate_name" = "customer" ] || [ "$crate_name" = "out" ]; then
                echo "⏭️  Skipping excluded crate: $crate_name"
                continue
            fi
            wasm_file="${crate_name//-/_}_s.wasm"
            echo "📦 Pushing $crate_name to registry..."
            cd "$dir"
            wash push --insecure localhost:5000/pipestack/${crate_name}:0.0.1 ./build/${wasm_file}
            cd - > /dev/null
        fi
    done

# Deploys an example from `examples/*`. Pass the example ID, e.g. 01 or 02, as a parameter
wash-deploy-example example: (wash-run-all "build")
    #!/usr/bin/env bash
    example_dir="examples/{{example}}*"
    found_dir=""

    for dir in $example_dir; do
        if [ -d "$dir" ]; then
            found_dir="$dir"
            break
        fi
    done

    if [ -z "$found_dir" ]; then
        echo "❌ No example directory found matching: {{example}}"
        exit 1
    fi

    echo "🚀 Running task 'deploy' in $found_dir ..."
    just --justfile "$found_dir/justfile" --working-directory "$found_dir" pipeline-deploy


wash-up:
    wash up --allowed-insecure localhost:5000 -d

wash-down:
    wash down --purge-jetstream all

wash-logs:
    tail -f ~/.wash/downloads/wasmcloud.log