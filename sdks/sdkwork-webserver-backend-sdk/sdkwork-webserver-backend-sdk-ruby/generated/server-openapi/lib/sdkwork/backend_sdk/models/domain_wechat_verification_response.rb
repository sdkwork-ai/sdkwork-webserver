module Sdkwork
  module BackendSdk
    module Models
      class DomainWechatVerificationResponse
              attr_accessor :root_domain_id, :file_name, :content, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @root_domain_id = attributes['rootDomainId']
                @file_name = attributes['fileName']
                @content = attributes['content']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'rootDomainId' => @root_domain_id,
                  'fileName' => @file_name,
                  'content' => @content,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end
