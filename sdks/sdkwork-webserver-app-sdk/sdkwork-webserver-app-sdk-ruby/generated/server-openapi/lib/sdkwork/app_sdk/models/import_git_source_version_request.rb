module Sdkwork
  module AppSdk
    module Models
      class ImportGitSourceVersionRequest
              attr_accessor :version_tag, :repository_url, :git_ref

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @version_tag = attributes['versionTag']
                @repository_url = attributes['repositoryUrl']
                @git_ref = attributes['gitRef']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'versionTag' => @version_tag,
                  'repositoryUrl' => @repository_url,
                  'gitRef' => @git_ref,
                }
              end
            end
    end
  end
end
