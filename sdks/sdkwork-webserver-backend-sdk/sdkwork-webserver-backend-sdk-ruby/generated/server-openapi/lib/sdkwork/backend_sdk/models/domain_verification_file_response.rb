module Sdkwork
  module BackendSdk
    module Models
      class DomainVerificationFileResponse
              attr_accessor :id, :root_domain_id, :platform, :file_name, :content, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @root_domain_id = attributes['rootDomainId']
                @platform = attributes['platform']
                @file_name = attributes['fileName']
                @content = attributes['content']
                @created_at = attributes['createdAt']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'rootDomainId' => @root_domain_id,
                  'platform' => @platform,
                  'fileName' => @file_name,
                  'content' => @content,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end
