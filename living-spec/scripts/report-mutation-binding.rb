#!/usr/bin/env ruby
require 'digest'
require 'json'

if ARGV.length != 2
  warn 'usage: report-mutation-binding.rb <status.json> <mutation.json>'
  exit 64
end

status_path, mutation_path = ARGV
expected = nil
actual = nil
begin
  status = JSON.parse(File.read(status_path))
  expected = status.dig('inputs', 'mutation_digest')
  actual = "sha256:#{Digest::SHA256.hexdigest(File.binread(mutation_path))}"
rescue StandardError
  # Verify reports unavailable or malformed bindings without turning them into
  # a failure. Mutation validation belongs to the explicit mutation score path.
end

if expected && expected == actual
  puts 'mutation byte binding: current (digest matches; contents are not validated by verify)'
else
  puts 'mutation byte binding: stale or unrecorded (run make living-spec-score-mutation to inspect)'
end
