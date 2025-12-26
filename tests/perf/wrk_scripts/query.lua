wrk.method = "GET"
wrk.path = "/search?q=test&page=1&limit=20&sort=date&filter=active"
wrk.headers["Content-Type"] = "application/json"
