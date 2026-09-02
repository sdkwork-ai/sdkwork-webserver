# frozen_string_literal: true

require_relative 'lib/sdkwork/backend_sdk/version'

Gem::Specification.new do |spec|
  spec.name = 'sdkwork-web-backend-sdk'
  spec.version = Sdkwork::BackendSdk::VERSION
  spec.authors = ['SDKWork Team']
  spec.summary = 'sdkwork-web-backend-sdk Ruby SDK'
  spec.description = 'sdkwork-web-backend-sdk Ruby SDK'
  spec.license = 'MIT'
  spec.required_ruby_version = '>= 3.0'
  spec.files = Dir.glob('lib/**/*') + ['README.md', 'sdkwork-web-backend-sdk.gemspec']
  spec.require_paths = ['lib']
  spec.add_dependency 'faraday', '~> 2.9'
  spec.metadata['homepage_uri'] = 'https://github.com/sdkwork/spring-ai-plus'
  spec.metadata['source_code_uri'] = 'https://github.com/sdkwork/spring-ai-plus'
end
