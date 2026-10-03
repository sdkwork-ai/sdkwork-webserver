module Sdkwork
  module BackendSdk
    module Models
      class CreateRootDomainRequest
              attr_accessor :hostname, :cloud_account_id

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @hostname = attributes['hostname']
                @cloud_account_id = attributes['cloudAccountId']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'hostname' => @hostname,
                  'cloudAccountId' => @cloud_account_id,
                }
              end
            end
    end
  end
end
