module Sdkwork
  module BackendSdk
    module Models
      class DnsAccountResponse
              attr_accessor :account_id, :provider, :zone_apex

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @account_id = attributes['accountId']
                @provider = attributes['provider']
                @zone_apex = attributes['zoneApex']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'accountId' => @account_id,
                  'provider' => @provider,
                  'zoneApex' => @zone_apex,
                }
              end
            end
    end
  end
end
