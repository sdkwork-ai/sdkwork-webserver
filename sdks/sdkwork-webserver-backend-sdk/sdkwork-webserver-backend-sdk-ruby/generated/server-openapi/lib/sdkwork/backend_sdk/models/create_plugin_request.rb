module Sdkwork
  module BackendSdk
    module Models
      class CreatePluginRequest
              attr_accessor :plugin_key, :display_name, :summary, :version, :category_id, :supported_host_tools, :contributed_capabilities, :source_kind, :git_repository, :git_ref, :artifact_ref, :checksum_sha256, :archive_file_name, :status

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @plugin_key = attributes['pluginKey']
                @display_name = attributes['displayName']
                @summary = attributes['summary']
                @version = attributes['version']
                @category_id = attributes['categoryId']
                @supported_host_tools = attributes['supportedHostTools'].is_a?(Array) ? attributes['supportedHostTools'].map { |item| item } : []
                @contributed_capabilities = attributes['contributedCapabilities'].is_a?(Array) ? attributes['contributedCapabilities'].map { |item| item } : []
                @source_kind = attributes['sourceKind']
                @git_repository = attributes['gitRepository']
                @git_ref = attributes['gitRef']
                @artifact_ref = attributes['artifactRef']
                @checksum_sha256 = attributes['checksumSha256']
                @archive_file_name = attributes['archiveFileName']
                @status = attributes['status']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'pluginKey' => @plugin_key,
                  'displayName' => @display_name,
                  'summary' => @summary,
                  'version' => @version,
                  'categoryId' => @category_id,
                  'supportedHostTools' => @supported_host_tools.is_a?(Array) ? @supported_host_tools.map { |item| item } : [],
                  'contributedCapabilities' => @contributed_capabilities.is_a?(Array) ? @contributed_capabilities.map { |item| item } : [],
                  'sourceKind' => @source_kind,
                  'gitRepository' => @git_repository,
                  'gitRef' => @git_ref,
                  'artifactRef' => @artifact_ref,
                  'checksumSha256' => @checksum_sha256,
                  'archiveFileName' => @archive_file_name,
                  'status' => @status,
                }
              end
            end
    end
  end
end
