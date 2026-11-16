module Sdkwork
  module BackendSdk
    module Models
      class CreateDomainVerificationFileRequest
              attr_accessor :platform, :file_name, :content

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @platform = attributes['platform']
                @file_name = attributes['fileName']
                @content = attributes['content']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'platform' => @platform,
                  'fileName' => @file_name,
                  'content' => @content,
                }
              end
            end
    end
  end
end
