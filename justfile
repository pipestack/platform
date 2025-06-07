# Builds all workspace members in crates/nodes/*
wash-build-all:
    #!/usr/bin/env bash
    for dir in crates/nodes/*/; do
        if [ -d "$dir" ]; then
            echo "👷 Building $dir ..."
            cd "$dir"
            wash build
            cd - > /dev/null
        fi
    done
    
# Deploys an example from `examples/*`. Pass the example ID, e.g. 01 or 02, as a parameter
wash-deploy-example example:
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
        echo "❌ No example directory found matching: $example"
        exit 1
    fi

    echo "🚀 Deploying example in $found_dir ..."
    cd "$found_dir"
    wash app delete example-pipeline
    wash app deploy local.wadm.yaml
    cd - > /dev/null

wash-up:
    wash up --allowed-insecure localhost:5000 -d

wash-down:
    wash down --purge-jetstream all

wash-logs-tail:
    tail -f ~/.wash/downloads/wasmcloud.log