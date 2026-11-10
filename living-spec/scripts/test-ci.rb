#!/usr/bin/env ruby
require 'fileutils'
require 'digest'
require 'json'
require 'open3'
require 'rbconfig'
require 'tmpdir'
require 'yaml'

ROOT = File.expand_path('../..', __dir__)
SCRIPT = File.join(ROOT, 'living-spec/scripts/score-ci.sh')
REFRESH_SCRIPT = File.join(ROOT, 'living-spec/scripts/refresh-dogfood.sh')
WORKFLOW = File.join(ROOT, '.github/workflows/living-spec.yml')

def assert(condition, message)
  raise message unless condition
end

def score_ready_requires_successful_complete_score
  workflow = YAML.load_file(WORKFLOW)
  triggers = workflow['on'] || workflow[true]
  assert(triggers && triggers.key?('workflow_dispatch'), 'workflow has a manual trigger')
  job = workflow.fetch('jobs').fetch('score')
  steps = job.fetch('steps')
  score = steps.find { |step| step['name'] == 'Score living spec' }
  assert(score, 'workflow has a named scoring step')
  assert(score.fetch('if').include?("steps.tests.outcome == 'success'") && score.fetch('if').include?("steps.tests.outcome == 'failure'") && score.fetch('if').include?('!cancelled()'), 'scoring runs after the test step whether tests pass or fail')
  assert(!score['continue-on-error'], 'scoring failure remains a job failure')
  assert(score.fetch('run').include?('living-spec/scripts/score-ci.sh'), 'workflow delegates scoring to the tested script')
  score_upload = steps.find { |step| step.dig('with', 'name') == 'living-spec-score' }
  assert(score_upload, 'workflow has score artifact upload')
  assert(score_upload.fetch('if').include?("steps.score.outputs.score_ready == 'true'"), 'score upload requires a fresh emitted status')
  assert(score_upload.dig('with', 'path').include?('living-spec-score/score/spec/status.json'), 'score upload reads isolated status')
  assert(!score_upload.dig('with', 'path').include?('living-spec/living-spec/status.json'), 'score upload excludes committed status')
  assert(steps.any? { |step| step['name'] == 'Run CI regressions' && step['run'].include?('living-spec/scripts/test-ci.rb') }, 'CI executes this regression')
  ruby_setup_index = steps.index { |step| step.dig('uses') == 'ruby/setup-ruby@v1' && step.dig('with', 'ruby-version') == '3.4' }
  regression_index = steps.index { |step| step['name'] == 'Run CI regressions' }
  assert(ruby_setup_index && ruby_setup_index < regression_index, 'CI installs the declared Ruby runtime before running regressions')
  diag_upload = steps.find { |step| step.dig('with', 'name') == 'living-spec-diagnostics' }
  assert(diag_upload && diag_upload.fetch('if').include?('always()'), 'diagnostics upload even when prior gates fail')
  assert(diag_upload.dig('with', 'path').include?('test-output.txt'), 'diagnostics include captured test output')
  assert(diag_upload.dig('with', 'path').include?('living-spec-diagnostics'), 'diagnostics use a separate artifact path')
  record = steps.find { |step| step['name'] == 'Record diagnostics' }
  assert(record && record.fetch('if').include?('always()') && record.fetch('run').include?('workflow-outcomes.txt'), 'workflow records earlier gate outcomes before artifact upload')
  assert(steps.index(record) < steps.index(diag_upload), 'diagnostics are recorded before upload')

  FileUtils.mkdir_p(File.join(ROOT, 'living-spec/target'))
  Dir.mktmpdir('living-spec-ci-regression', File.join(ROOT, 'living-spec/target')) do |tmp|
    repo = File.join(tmp, 'repo')
    source = File.join(repo, 'living-spec/living-spec')
    FileUtils.mkdir_p(source)
    %w[spec.md annotations.json history.jsonl].each { |name| File.write(File.join(source, name), "fixture-#{name}\n") }
    File.write(File.join(source, 'status.json'), '{"commit":"old-commit"}')
    File.write(File.join(source, 'spec.html'), 'historical html')
    results = File.join(tmp, 'test-output.txt')
    File.write(results, "test suite ... ok\ntest result: ok\n")
    fake_cargo = File.join(tmp, 'fake-cargo')
    File.write(fake_cargo, <<~SH)
      #!/bin/sh
      set -eu
      score_dir=
      html=
      commit=
      while [ "$#" -gt 0 ]; do
        case "$1" in
          score) score_dir=$2; shift 2 ;;
          --html) html=$2; shift 2 ;;
          --commit) commit=$2; shift 2 ;;
          *) shift ;;
        esac
      done
      test -n "$score_dir" && test -n "$html" && test -n "$commit"
      test ! -e "$score_dir/status.json"
      test ! -e "$score_dir/spec.html"
      test "$(cat "$score_dir/spec.md")" = "fixture-spec.md"
      test "$(cat "$score_dir/annotations.json")" = "fixture-annotations.json"
      test "$(cat "$score_dir/history.jsonl")" = "fixture-history.jsonl"
      echo "fake scorer ran: ${FAKE_SCORE_MODE:-green}"
      case "${FAKE_SCORE_MODE:-green}" in
        green|broken)
          printf '{"commit":"%s"}' "$commit" > "$score_dir/status.json"
          printf 'new history\n' >> "$score_dir/history.jsonl"
          printf '<html>new score</html>' > "$html"
          test "${FAKE_SCORE_MODE:-green}" = green && exit 0
          exit 2
          ;;
        stale)
          printf '{"commit":"old-commit"}' > "$score_dir/status.json"
          printf '<html>old score</html>' > "$html"
          exit 0
          ;;
        missing) exit 0 ;;
        partial|partial-html|status-only)
          printf '{"commit":"%s"}' "$commit" > "$score_dir/status.json"
          if test "$FAKE_SCORE_MODE" = partial-html; then
            printf '<html>unfinished score</html>' > "$html"
          fi
          test "$FAKE_SCORE_MODE" = status-only && exit 0
          exit 1
          ;;
        *) exit 1 ;;
      esac
    SH
    FileUtils.chmod(0o755, fake_cargo)

    run = lambda do |label, mode, expected_exit, expected_ready|
      artifacts = File.join(tmp, "#{label}-artifacts")
      diagnostics = File.join(tmp, "#{label}-diagnostics")
      outputs = File.join(tmp, "#{label}-outputs")
      env = { 'GITHUB_OUTPUT' => outputs, 'FAKE_SCORE_MODE' => mode }
      ok = system(env, SCRIPT, repo, 'current-commit', results, artifacts, diagnostics, fake_cargo, out: File::NULL, err: File::NULL)
      code = ok ? 0 : $?.exitstatus
      assert(code == expected_exit, "#{label} preserves scorer exit #{expected_exit}, got #{code}")
      assert(File.read(File.join(diagnostics, 'test-output.txt')) == File.read(results), "#{label} captures test output")
      output = File.read(outputs)
      ready = output.include?('score_ready=true')
      assert(ready == [0, 2].include?(expected_exit), "#{label} sets score readiness only for a new status, got #{output.inspect}")
      status_emitted = %w[green broken stale partial partial-html status-only].include?(mode)
      html_emitted = %w[green broken stale partial-html].include?(mode)
      assert(File.exist?(File.join(artifacts, 'score/spec/status.json')) == status_emitted, "#{label} status emission matches scorer behavior")
      assert(File.exist?(File.join(artifacts, 'score/spec.html')) == html_emitted, "#{label} HTML emission matches scorer behavior")
      assert(File.read(File.join(diagnostics, 'score.log')).include?("fake scorer ran: #{mode}"), "#{label} retains scoring diagnostics")
      assert(ready == expected_ready, "#{label} readiness is #{expected_ready}, got #{output.inspect}")
    end

    run.call('green', 'green', 0, true)
    run.call('broken', 'broken', 2, true)
    run.call('incomplete', 'incomplete', 1, false)
    run.call('stale', 'stale', 1, false)
    run.call('missing', 'missing', 1, false)
    run.call('partial', 'partial', 1, false)
    run.call('partial-html', 'partial-html', 1, false)
    run.call('status-only', 'status-only', 1, false)
  end

end

def mutation_job_is_manual_and_uploads_evidence
  workflow = YAML.load_file(WORKFLOW)
  job = workflow.fetch('jobs').fetch('mutation')
  assert(job.fetch('if').include?("github.event_name == 'workflow_dispatch'"), 'mutation job is manual-only')
  assert(job.dig('env', 'CARGO_TERM_COLOR') == 'never' && job.dig('env', 'NO_COLOR') == '1', 'mutation job disables cargo color')
  steps = job.fetch('steps')
  assert(steps.any? { |step| step['run'] == 'make living-spec-mutate' }, 'manual job runs mutation command')
  fetch_index = steps.index { |step| step['name'] == 'Fetch mutation dependencies' && step['run'] == 'cargo fetch --locked --manifest-path living-spec/Cargo.toml' }
  mutate_index = steps.index { |step| step['run'] == 'make living-spec-mutate' }
  assert(fetch_index && mutate_index && fetch_index < mutate_index, 'manual job fetches locked dependencies before offline mutation checks')
  assert(steps.any? { |step| step['run'] == 'make living-spec-score-mutation' }, 'manual job scores mutation evidence')
  upload = steps.find { |step| step.dig('with', 'name') == 'living-spec-mutation' }
  assert(upload && upload.dig('with', 'path') == 'living-spec/living-spec/mutation.json', 'manual job uploads mutation file')
  makefile = File.read(File.join(ROOT, 'Makefile'))
  assert(makefile.lines.find { |line| line.start_with?('verify:') } == "verify: living-spec-score\n", 'mutation stays outside verify')
  score_lines = makefile.lines.drop_while { |line| !line.start_with?('living-spec-score:') }
  score_recipe = score_lines.drop(1).take_while { |line| line.start_with?("\t") }
  assert(score_recipe.none? { |line| line.include?('--mutation ') }, 'verify does not ingest mutation evidence')
  assert(score_recipe.any? { |line| line.include?('report-mutation-binding.rb') }, 'verify reports mutation byte freshness')
end

def mutation_binding_report_is_informational
  script = File.join(ROOT, 'living-spec/scripts/report-mutation-binding.rb')
  FileUtils.mkdir_p(File.join(ROOT, 'living-spec/target'))
  Dir.mktmpdir('living-spec-mutation-binding', File.join(ROOT, 'living-spec/target')) do |tmp|
    status = File.join(tmp, 'status.json')
    mutation = File.join(tmp, 'mutation.json')
    File.write(mutation, '{malformed')
    digest = "sha256:#{Digest::SHA256.hexdigest(File.binread(mutation))}"
    File.write(status, JSON.generate('inputs' => { 'mutation_digest' => digest }))
    output, error, process = Open3.capture3(RbConfig.ruby, script, status, mutation)
    assert(process.success? && error.empty? && output.include?('current') && output.include?('contents are not validated'), 'matching bytes report current without parsing payload')

    File.write(status, JSON.generate('inputs' => { 'mutation_digest' => "sha256:#{'0' * 64}" }))
    output, _error, process = Open3.capture3(RbConfig.ruby, script, status, mutation)
    assert(process.success? && output.include?('stale or unrecorded'), 'changed bytes report stale without failing')

    File.write(status, '{malformed status')
    output, _error, process = Open3.capture3(RbConfig.ruby, script, status, mutation)
    assert(process.success? && output.include?('stale or unrecorded'), 'unreadable binding stays informational')
  end
end

def refresh_records_broken_status
  makefile = File.read(File.join(ROOT, 'Makefile'))
  assert(makefile.include?('living-spec/scripts/refresh-dogfood.sh'), 'refresh target delegates to staged refresh script')
  assert(!makefile.match?(/^\s*rm -f living-spec\/living-spec\/(?:history|status)\.json/), 'refresh target preserves artifacts before scoring')

  Dir.mktmpdir('living-spec-refresh-regression', File.join(ROOT, 'living-spec/target')) do |tmp|
    repo = File.join(tmp, 'repo')
    source = File.join(repo, 'living-spec/living-spec')
    FileUtils.mkdir_p(source)
    %w[spec.md annotations.json].each { |name| File.write(File.join(source, name), "fixture-#{name}\n") }
    File.write(File.join(source, 'history.jsonl'), "prior history\n")
    File.write(File.join(source, 'status.json'), 'prior status')
    File.write(File.join(source, 'spec.html'), 'prior html')
    results = File.join(tmp, 'test-output.txt')
    File.write(results, "test suite ... ok\ntest result: ok\n")
    fake_cargo = File.join(tmp, 'fake-cargo')
    File.write(fake_cargo, <<~SH)
      #!/bin/sh
      set -eu
      score_dir=
      html=
      mutation=
      while [ "$#" -gt 0 ]; do
        case "$1" in
          score) score_dir=$2; shift 2 ;;
          --html) html=$2; shift 2 ;;
          --mutation) mutation=$2; shift 2 ;;
          *) shift ;;
        esac
      done
      printf '%s\n' "$mutation" >> "${FAKE_MUTATION_LOG:?}"
      test -n "$score_dir" && test -n "$html"
      case "$(cat "$score_dir/history.jsonl")" in
        "prior history"*) ;;
        *) exit 1 ;;
      esac
      printf '{"commit":"new","inputs":{"results_file":"staged"}}' > "$score_dir/status.json"
      printf 'new annotations' > "$score_dir/annotations.json"
      echo 'new history' >> "$score_dir/history.jsonl"
      printf 'new html' > "$html"
      test "${FAKE_REFRESH_MODE:-success}" = success && exit 0
      test "${FAKE_REFRESH_MODE:-success}" = broken && exit 2
      exit 1
    SH
    FileUtils.chmod(0o755, fake_cargo)
    mutation_log = File.join(tmp, 'mutation-args')

    { 'failure' => 1 }.each do |mode, exit_code|
      system({ 'FAKE_REFRESH_MODE' => mode, 'FAKE_MUTATION_LOG' => mutation_log }, 'bash', REFRESH_SCRIPT, repo, results, fake_cargo, out: File::NULL, err: File::NULL)
      assert($?.exitstatus == exit_code, "#{mode} staged score returns its error")
      { 'annotations.json' => "fixture-annotations.json\n", 'history.jsonl' => "prior history\n", 'status.json' => 'prior status', 'spec.html' => 'prior html' }.each do |name, expected|
        assert(File.read(File.join(source, name)) == expected, "#{mode} score preserves committed #{name}")
      end
    end

    ok = system({ 'FAKE_REFRESH_MODE' => 'broken', 'FAKE_MUTATION_LOG' => mutation_log }, 'bash', REFRESH_SCRIPT, repo, results, fake_cargo, out: File::NULL, err: File::NULL)
    assert(!ok && $?.exitstatus == 2, 'refresh_records_broken_status returns broken score')
    assert(File.read(File.join(source, 'history.jsonl')) == "prior history\nnew history\n", 'successful refresh appends history')
    status = JSON.parse(File.read(File.join(source, 'status.json')))
    assert(status['commit'] == 'new', 'successful refresh replaces status')
    assert(File.read(File.join(source, 'annotations.json')) == 'new annotations', 'successful refresh keeps the annotations bound by its status')
    assert(File.expand_path(status.dig('inputs', 'results_file'), source) == results, 'refresh result locator remains valid after staging')
    assert(File.read(File.join(source, 'spec.html')) == 'new html', 'successful refresh replaces page')
    assert(File.read(mutation_log).lines.all? { |line| line == "\n" }, 'refresh omits mutation option when evidence is absent')

    File.write(File.join(source, 'mutation.json'), '{"fixture":true}')
    ok = system({ 'FAKE_REFRESH_MODE' => 'success', 'FAKE_MUTATION_LOG' => mutation_log }, 'bash', REFRESH_SCRIPT, repo, results, fake_cargo, out: File::NULL, err: File::NULL)
    assert(ok, 'refresh succeeds when mutation evidence is present')
    assert(File.read(mutation_log).lines.last&.end_with?("/spec/mutation.json\n"), 'refresh scores the staged mutation file')
  end
end

score_ready_requires_successful_complete_score
puts 'score_ready_requires_successful_complete_score passed.'
mutation_job_is_manual_and_uploads_evidence
puts 'mutation_job_is_manual_and_uploads_evidence passed.'
mutation_binding_report_is_informational
puts 'mutation_binding_report_is_informational passed.'
refresh_records_broken_status
puts 'refresh_records_broken_status passed.'
