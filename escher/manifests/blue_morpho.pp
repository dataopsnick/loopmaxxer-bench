# == Class: profile::blue_morpho
#
# Manages the Project Blue Morpho (Escher) autonomous container and/or
# scheduled wake-from-sleep cron triggers targeting:
# https://tensorpros-project-blue-morpho.hf.space
#
class profile::blue_morpho (
  String                    $hf_space_url       = 'https://tensorpros-project-blue-morpho.hf.space',
  String                    $hf_space_id        = 'tensorpros/project-blue-morpho',
  String                    $preferred_model    = 'project-blue-morpho',
  String                    $headless_command   = '/review-prs',
  Boolean                   $manage_container   = true,
  Boolean                   $enable_wake_cron   = true,
  Optional[Sensitive[String]] $ocr_bootstrap_json = undef,
  Optional[Sensitive[String]] $github_token       = undef,
  Optional[Sensitive[String]] $hf_token           = undef,
  Optional[Sensitive[String]] $cf_tunnel_token    = undef,
  String                    $image_name         = 'ghcr.io/dataopsnick/project-blue-morpho:latest',
) {

  # 1. Deploy the headless verification & wake script on the Puppet node
  file { '/usr/local/bin/blue-morpho-wake':
    ensure  => file,
    owner   => 'root',
    group   => 'root',
    mode    => '0755',
    content => @(EOT)
      #!/usr/bin/env bash
      set -euo pipefail
      SPACE_URL="${1:-https://tensorpros-project-blue-morpho.hf.space}"
      CMD="${2:-/review-prs}"
      MODEL="${3:-project-blue-morpho}"

      # Wake Space if sleeping (up to 60s)
      for i in {1..20}; do
        CODE=$(curl -s -o /dev/null -w "%{http_code}" "$SPACE_URL/v1/models" || echo "000")
        [ "$CODE" = "200" ] && break
        sleep 3
      done

      # Discover active Cloudflared tunnel if available, otherwise fall back to HF Space URL
      TUNNEL_URL=$(curl -s "$SPACE_URL/v1/tunnel" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("tunnel_url") or "")' 2>/dev/null || true)
      TARGET_URL="${TUNNEL_URL:-$SPACE_URL}"

      curl -sN -X POST "${TARGET_URL%/}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer sk-dummy-key" \
        -d "{\"model\": \"$MODEL\", \"messages\": [{\"role\": \"user\", \"content\": \"$CMD\"}]}"
      | EOT
  }

  # 2. Scheduled Cron Job to wake the HF Space and run PR reviews (every 8 hours per AGENTS.md budget bound)
  if $enable_wake_cron {
    cron { 'blue-morpho-hf-space-wake-and-review':
      ensure  => present,
      command => "/usr/local/bin/blue-morpho-wake '${hf_space_url}' '${headless_command}' '${preferred_model}' >> /var/log/blue-morpho-cron.log 2>&1",
      user    => 'root',
      hour    => ['0', '8', '16'],
      minute  => '0',
      require => File['/usr/local/bin/blue-morpho-wake'],
    }
  }

  # 3. Optional Self-Hosted Docker Container with Cloudflared DDNS
  if $manage_container {
    include docker

    file { '/etc/blue-morpho.env':
      ensure  => file,
      owner   => 'root',
      group   => 'root',
      mode    => '0600',
      content => inline_epp(@(EOT)),
        SPACE_ID=<%= $hf_space_id %>
        SPACE_HOST=tensorpros-project-blue-morpho.hf.space
        ESCHER_URL=<%= $hf_space_url %>
        PREFERRED_MODEL=<%= $preferred_model %>
        ENABLE_QUICK_TUNNEL=true
        AUTO_REGISTER_WEBHOOK=true
        <% if $ocr_bootstrap_json { -%>
        OCR_BOOTSTRAP_JSON=<%= $ocr_bootstrap_json.unwrap %>
        <% } -%>
        <% if $github_token { -%>
        GITHUB_TOKEN=<%= $github_token.unwrap %>
        <% } -%>
        <% if $hf_token { -%>
        HF_TOKEN=<%= $hf_token.unwrap %>
        <% } -%>
        <% if $cf_tunnel_token { -%>
        CF_TUNNEL_TOKEN=<%= $cf_tunnel_token.unwrap %>
        <% } -%>
        | EOT
    }

    docker::run { 'project-blue-morpho':
      image            => $image_name,
      env_file         => ['/etc/blue-morpho.env'],
      ports            => ['127.0.0.1:7860:7860'],
      restart_service  => true,
      pull_on_start    => true,
      extra_parameters => ['--memory=4g'],
      require          => File['/etc/blue-morpho.env'],
    }
  }
}