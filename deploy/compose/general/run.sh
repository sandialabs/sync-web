#!/bin/sh

if [ -z "$SECRET" ]; then
    echo Must set the SECRET variable""
    exit 1
fi

if [ -z "$WINDOW" ]; then
    WINDOW="#f"
fi

if [ -z "$JOURNAL_UPDATE" ]; then
    JOURNAL_UPDATE=0
fi

resolve_lisp_file() {
    filename="$1"

    if [ -n "$LISP_DIR" ]; then
        path="$LISP_DIR/$filename"
        if [ ! -f "$path" ]; then
            echo "Missing required Lisp file: $path"
            exit 1
        fi
        echo "$path"
        return 0
    fi

    if [ ! -f "$filename" ]; then
        echo "Missing required Lisp file: $filename"
        exit 1
    fi
    echo "$filename"
}

build_admins_list() {
    result=""
    OLD_IFS="$IFS"
    IFS=","
    for name in ${INTERFACE_ADMINS:-}; do
        result="$result '$name"
    done
    IFS="$OLD_IFS"
    echo "(list$result)"
}

run_startup() {
    clear_flag="$1"
    root=$( cat "$(resolve_lisp_file root.scm)" )
    standard=$( cat "$(resolve_lisp_file standard.scm)" )
    chain=$( cat "$(resolve_lisp_file log-chain.scm)" )
    tree=$( cat "$(resolve_lisp_file tree.scm)" )
    ledger=$( cat "$(resolve_lisp_file ledger.scm)" )
    interface=$( cat "$(resolve_lisp_file interface.scm)" )
    admins=$( build_admins_list )
    expr="($interface $clear_flag \"$SECRET\" \"$SECRET\" $admins $WINDOW $root '$standard '$chain '$tree '$ledger)"
    if [ "$clear_flag" = "#f" ]; then
        expr="(*eval* \"$SECRET\" $expr)"
    fi
    RUST_LOG=$RUST_LOG ./journal-sdk -e "$expr" -d database
}

if [ -d database ] && [ -n "$(find database -mindepth 1 -print -quit 2>/dev/null)" ]; then
    if [ "$JOURNAL_UPDATE" = "1" ]; then
        run_startup "#f"
    fi
else
    run_startup "#t"
fi

step="(*step* \"$SECRET\")"
RUST_LOG=$RUST_LOG ./journal-sdk -p 80 -c $PERIOD -s "$step" -d database
