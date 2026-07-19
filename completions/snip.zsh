#compdef snip

_snip() {
    local -a commands
    commands=(
        'init:Create or detect .snips file'
        'add:Add a new snippet'
        'rm:Remove a snippet'
        'edit:Open .snips in $EDITOR'
        'list:List snippets (default)'
        'run:Execute a snippet'
        'import:Import snippets from another project'
        'doctor:Validate snippets and report issues'
        'completions:Generate shell completions'
        'hook:Print shell integration code'
        'suggest:Suggest snippets from shell history'
        'explain:Explain what a snippet command does'
        'stale:Detect unused or outdated snippets'
        'setup:Team onboarding wizard'
    )

    local -a snippet_names
    snippet_names=(${(f)"$(snip _complete snippets 2>/dev/null)"})

    if (( CURRENT == 2 )); then
        _describe 'command' commands
        _describe 'snippets' snippet_names
    else
        case "$words[2]" in
            run|explain|rm)
                _describe 'snippets' snippet_names
                ;;
            completions)
                _values 'shells' bash zsh fish nushell elvish powershell
                ;;
            doctor)
                _values 'flags' '--fix'
                ;;
            list)
                _values 'flags' '--json' '--format' '--section'
                ;;
            stale)
                _values 'flags' '--fix' '--json'
                ;;
        esac
    fi
}

_snip "$@"