object "SimpleStore" {
  code {
    datacopy(0, dataoffset("SimpleStore_deployed"), datasize("SimpleStore_deployed"))
    return(0, datasize("SimpleStore_deployed"))
  }
  object "SimpleStore_deployed" {
    code {
      calldatacopy(0, 0, 36) // write calldata to memory
    }
  }
}
