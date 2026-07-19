# Disable file completions for snip
complete -c snip -f

# Subcommands
complete -c snip -n __fish_use_subcommand -a init -d 'Create or detect .snips file'
complete -c snip -n __fish_use_subcommand -a add -d 'Add a new snippet'
complete -c snip -n __fish_use_subcommand -a rm -d 'Remove a snippet'
complete -c snip -n __fish_use_subcommand -a edit -d 'Open .snips in $EDITOR'
complete -c snip -n __fish_use_subcommand -a list -d 'List snippets'
complete -c snip -n __fish_use_subcommand -a run -d 'Execute a snippet'
complete -c snip -n __fish_use_subcommand -a import -d 'Import snippets from another project'
complete -c snip -n __fish_use_subcommand -a doctor -d 'Validate snippets'
complete -c snip -n __fish_use_subcommand -a completions -d 'Generate shell completions'
complete -c snip -n __fish_use_subcommand -a hook -d 'Print shell integration code'
complete -c snip -n __fish_use_subcommand -a suggest -d 'Suggest snippets from history'
complete -c snip -n __fish_use_subcommand -a explain -d 'Explain a snippet command'
complete -c snip -n __fish_use_subcommand -a stale -d 'Detect unused snippets'
complete -c snip -n __fish_use_subcommand -a setup -d 'Team onboarding wizard'

# Dynamic snippet completions for run/rm/explain
complete -c snip -n '__fish_seen_subcommand_from run rm explain' -a '(snip _complete snippets 2>/dev/null)'

# Flag completions
complete -c snip -n '__fish_seen_subcommand_from completions' -a bash zsh fish nushell elvish powershell
complete -c snip -n '__fish_seen_subcommand_from doctor' -l fix
complete -c snip -n '__fish_seen_subcommand_from list' -l json -l format -l section
complete -c snip -n '__fish_seen_subcommand_from stale' -l fix -l json