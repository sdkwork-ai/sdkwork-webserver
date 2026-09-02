module Sdkwork
  module AppSdk
    class SdkConfig
      attr_accessor :base_url, :timeout, :headers, :connection_options

      def initialize(base_url: 'http://localhost:3800', timeout: 30, headers: {}, connection_options: {})
        @base_url = base_url
        @timeout = timeout
        @headers = headers || {}
        @connection_options = connection_options || {}
      end
    end
  end
end
