# Runs a wash command in all workspace members in crates/nodes/*
wash-run-all command="build":
    #!/usr/bin/env bash
    for dir in crates/nodes/*/; do
        if [ -d "$dir" ]; then
            crate_name=$(basename "$dir")
            just wash-run-one ${crate_name} {{command}}
        fi
    done

# Runs a wash command in a workspace member in crates/nodes/*
wash-run-one node command="build":
    #!/usr/bin/env bash
    if [ -d "crates/nodes/{{node}}" ]; then
        echo "👷 Running command '{{command}}' in {{node}} ..."
        cd "crates/nodes/{{node}}"
        wash wit deps
        wash {{command}}
        cd - > /dev/null
    fi

# Pushes all workspace members in crates/nodes/* to the registry
wash-push-all:
    #!/usr/bin/env bash
    for dir in crates/nodes/*/; do
        if [ -d "$dir" ]; then
            crate_name=$(basename "$dir")
            just wash-push-one ${crate_name}
        fi
    done

# Pushes a workspace member in crates/nodes/* to the registry
wash-push-one node: (wash-run-one node "build")
    #!/usr/bin/env bash
    if [ -d "crates/nodes/{{node}}" ]; then
        crate_name=$(basename "{{node}}" | sed 's/-/_/g')
        if [ "$crate_name" = "customer" ] || [ "$crate_name" = "out" ]; then
            echo "⏭️  Skipping excluded crate: $crate_name"
            continue
        fi
        wasm_file="${crate_name//-/_}_s.wasm"
        echo "📦 Pushing $crate_name to registry..."
        cd "crates/nodes/{{node}}"
        version=$(grep '^version' Cargo.toml | cut -d'"' -f2)
        wash push --insecure localhost:5000/nodes/${crate_name}:${version} ./build/${wasm_file}
        cd - > /dev/null
    fi

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


wash-up lattice="default":
    wash up --log-level trace --allowed-insecure localhost:5000 --lattice {{lattice}} -d

wash-down:
    wash down --purge-jetstream all --all
    wash drain all

wash-restart:
    just wash-down
    just wash-up

wash-logs:
    tail -f ~/.wash/downloads/wasmcloud.log